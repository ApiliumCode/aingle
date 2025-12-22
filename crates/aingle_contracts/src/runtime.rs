//! Contract runtime and WASM execution
//!
//! Provides sandboxed execution environment for contracts.

use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

use crate::contract::{Contract, ContractInstance, FunctionType};
use crate::error::{ContractError, Result};
use crate::storage::{ContractStorage, MemoryStorage, StorageKey, StorageValue};
use crate::types::{Address, CallResult, Event, Gas, StateChange};

/// Execution context for contract calls
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Caller address
    pub caller: Address,
    /// Contract address
    pub contract: Address,
    /// Value sent with call
    pub value: u64,
    /// Gas limit
    pub gas_limit: Gas,
    /// Block number
    pub block_number: u64,
    /// Block timestamp
    pub block_timestamp: u64,
    /// Call depth (for reentrancy detection)
    pub depth: u32,
    /// Max call depth
    pub max_depth: u32,
}

impl ExecutionContext {
    /// Create new context
    pub fn new(caller: Address, contract: Address) -> Self {
        Self {
            caller,
            contract,
            value: 0,
            gas_limit: Gas::new(1_000_000), // Default 1M gas
            block_number: 0,
            block_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            depth: 0,
            max_depth: 10,
        }
    }

    /// Set value
    pub fn with_value(mut self, value: u64) -> Self {
        self.value = value;
        self
    }

    /// Set gas limit
    pub fn with_gas(mut self, gas: Gas) -> Self {
        self.gas_limit = gas;
        self
    }

    /// Increment depth (for nested calls)
    pub fn nested(&self) -> Result<Self> {
        if self.depth >= self.max_depth {
            return Err(ContractError::ReentrancyDetected(format!(
                "Max call depth {} exceeded",
                self.max_depth
            )));
        }
        let mut ctx = self.clone();
        ctx.depth += 1;
        Ok(ctx)
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new(Address::zero(), Address::zero())
    }
}

/// Contract runtime
pub struct ContractRuntime {
    /// Storage backend
    storage: Arc<dyn ContractStorage>,
    /// Deployed contracts
    contracts: HashMap<Address, ContractInstance>,
    /// Gas prices for operations
    gas_prices: GasPrices,
}

/// Gas prices for different operations
#[derive(Debug, Clone)]
pub struct GasPrices {
    /// Base cost for any call
    pub base_cost: u64,
    /// Cost per byte of input data
    pub per_byte: u64,
    /// Cost for storage read
    pub storage_read: u64,
    /// Cost for storage write
    pub storage_write: u64,
    /// Cost for emitting event
    pub event_emit: u64,
    /// Cost for logging
    pub log: u64,
}

impl Default for GasPrices {
    fn default() -> Self {
        Self {
            base_cost: 100,
            per_byte: 1,
            storage_read: 200,
            storage_write: 5000,
            event_emit: 375,
            log: 100,
        }
    }
}

impl ContractRuntime {
    /// Create new runtime with memory storage
    pub fn new() -> Result<Self> {
        Ok(Self {
            storage: Arc::new(MemoryStorage::new()),
            contracts: HashMap::new(),
            gas_prices: GasPrices::default(),
        })
    }

    /// Create with custom storage
    pub fn with_storage(storage: Arc<dyn ContractStorage>) -> Self {
        Self {
            storage,
            contracts: HashMap::new(),
            gas_prices: GasPrices::default(),
        }
    }

    /// Set gas prices
    pub fn with_gas_prices(mut self, prices: GasPrices) -> Self {
        self.gas_prices = prices;
        self
    }

    /// Deploy a contract
    pub fn deploy(
        &mut self,
        contract: Contract,
        deployer: Address,
        initial_state: serde_json::Value,
        _ctx: &ExecutionContext,
    ) -> Result<Address> {
        info!("Deploying contract: {}", contract.name);

        // Create instance
        let instance = ContractInstance::new(contract.clone(), deployer, initial_state.clone());
        let address = instance.address.clone();

        // Check if already deployed
        if self.contracts.contains_key(&address) {
            return Err(ContractError::ContractExists(address.to_hex()));
        }

        // Initialize storage with initial state
        self.init_contract_state(&address, &initial_state)?;

        // Run constructor if exists
        if contract.has_function("constructor") {
            debug!("Running constructor for {}", address);
            // Constructor would be called here with initial args
        }

        self.contracts.insert(address.clone(), instance);

        info!("Contract deployed at: {}", address);
        Ok(address)
    }

    /// Initialize contract state from JSON
    fn init_contract_state(&self, address: &Address, state: &serde_json::Value) -> Result<()> {
        if let serde_json::Value::Object(map) = state {
            for (key, value) in map {
                let storage_key = StorageKey::from_string(address.clone(), key);
                let storage_value = StorageValue::from_json(value)?;
                self.storage.set(&storage_key, storage_value)?;
            }
        }
        Ok(())
    }

    /// Call a contract function
    pub fn call(
        &self,
        address: &Address,
        function: &str,
        args: &[serde_json::Value],
        ctx: &mut ExecutionContext,
    ) -> Result<CallResult> {
        debug!("Calling {}.{} with {:?}", address, function, args);

        // Get contract
        let instance = self
            .contracts
            .get(address)
            .ok_or_else(|| ContractError::ContractNotFound(address.to_hex()))?;

        // Get function
        let func = instance
            .contract
            .get_function(function)
            .ok_or_else(|| ContractError::FunctionNotFound(function.to_string()))?;

        // Check permissions
        if func.function_type == FunctionType::Internal {
            return Err(ContractError::PermissionDenied(
                "Cannot call internal function".to_string(),
            ));
        }

        // Check value for non-payable
        if ctx.value > 0 && !func.is_payable() {
            return Err(ContractError::PermissionDenied(
                "Function is not payable".to_string(),
            ));
        }

        // Consume base gas
        let base_cost = self.gas_prices.base_cost + func.gas_cost;
        ctx.gas_limit
            .consume(base_cost)
            .map_err(|_| ContractError::OutOfGas {
                used: base_cost,
                limit: ctx.gas_limit.0 + base_cost,
            })?;

        // Execute (simplified - real impl would run WASM)
        let result = self.execute_function(instance, func.name.as_str(), args, ctx)?;

        Ok(result)
    }

    /// Execute contract function (simplified implementation)
    fn execute_function(
        &self,
        instance: &ContractInstance,
        function: &str,
        args: &[serde_json::Value],
        ctx: &mut ExecutionContext,
    ) -> Result<CallResult> {
        let mut result = CallResult::empty();
        let gas_start = ctx.gas_limit.remaining();

        // Simplified execution - in real impl, this would run WASM code
        match function {
            "get" | "balance_of" | "get_balance" => {
                // Generic getter
                if let Some(key) = args.first().and_then(|v| v.as_str()) {
                    let storage_key = StorageKey::from_string(instance.address.clone(), key);

                    ctx.gas_limit
                        .consume(self.gas_prices.storage_read)
                        .map_err(|_| ContractError::OutOfGas {
                            used: self.gas_prices.storage_read,
                            limit: ctx.gas_limit.0,
                        })?;

                    if let Some(value) = self.storage.get(&storage_key)? {
                        result.value = value.to_json()?;
                    }
                }
            }
            "set" | "transfer" | "mint" => {
                // Generic setter
                if args.len() >= 2 {
                    let key = args[0].as_str().unwrap_or("default");
                    let value = &args[1];

                    let storage_key = StorageKey::from_string(instance.address.clone(), key);

                    // Record old value for state change
                    let old_value = self
                        .storage
                        .get(&storage_key)?
                        .and_then(|v| v.to_json().ok());

                    ctx.gas_limit
                        .consume(self.gas_prices.storage_write)
                        .map_err(|_| ContractError::OutOfGas {
                            used: self.gas_prices.storage_write,
                            limit: ctx.gas_limit.0,
                        })?;

                    let storage_value = StorageValue::from_json(value)?;
                    self.storage.set(&storage_key, storage_value)?;

                    result.value = serde_json::json!(true);

                    // Record state change
                    result
                        .state_changes
                        .push(StateChange::set(key, old_value, value.clone()));

                    // Emit event
                    result.events.push(Event::new(
                        format!("{}Called", function),
                        serde_json::json!({
                            "key": key,
                            "value": value
                        }),
                    ));
                }
            }
            _ => {
                // Unknown function - return null
                result.value = serde_json::Value::Null;
            }
        }

        result.gas_used = gas_start - ctx.gas_limit.remaining();

        Ok(result)
    }

    /// Get contract at address
    pub fn get_contract(&self, address: &Address) -> Option<&ContractInstance> {
        self.contracts.get(address)
    }

    /// Check if address has contract
    pub fn has_contract(&self, address: &Address) -> bool {
        self.contracts.contains_key(address)
    }

    /// Get all deployed contracts
    pub fn list_contracts(&self) -> Vec<&Address> {
        self.contracts.keys().collect()
    }

    /// Read contract state
    pub fn read_state(&self, address: &Address, key: &str) -> Result<Option<serde_json::Value>> {
        let storage_key = StorageKey::from_string(address.clone(), key);
        if let Some(value) = self.storage.get(&storage_key)? {
            Ok(Some(value.to_json()?))
        } else {
            Ok(None)
        }
    }

    /// Get storage backend
    pub fn storage(&self) -> &Arc<dyn ContractStorage> {
        &self.storage
    }
}

impl Default for ContractRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create default runtime")
    }
}

/// Host functions available to contracts
pub mod host {
    use super::*;

    /// Log a message
    pub fn log(ctx: &ExecutionContext, level: &str, message: &str) {
        match level {
            "error" => tracing::error!(contract = %ctx.contract, "{}", message),
            "warn" => tracing::warn!(contract = %ctx.contract, "{}", message),
            "info" => tracing::info!(contract = %ctx.contract, "{}", message),
            _ => tracing::debug!(contract = %ctx.contract, "{}", message),
        }
    }

    /// Get current block number
    pub fn block_number(ctx: &ExecutionContext) -> u64 {
        ctx.block_number
    }

    /// Get current block timestamp
    pub fn block_timestamp(ctx: &ExecutionContext) -> u64 {
        ctx.block_timestamp
    }

    /// Get caller address
    pub fn caller(ctx: &ExecutionContext) -> &Address {
        &ctx.caller
    }

    /// Get current contract address
    pub fn address(ctx: &ExecutionContext) -> &Address {
        &ctx.contract
    }

    /// Get value sent with call
    pub fn value(ctx: &ExecutionContext) -> u64 {
        ctx.value
    }

    /// Compute hash
    pub fn hash(data: &[u8]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ContractBuilder;

    #[test]
    fn test_deploy_contract() {
        let mut runtime = ContractRuntime::new().unwrap();

        let contract = ContractBuilder::new("test")
            .function("get", vec!["key"])
            .function("set", vec!["key", "value"])
            .build()
            .unwrap();

        let deployer = Address::derive("deployer");
        let ctx = ExecutionContext::new(deployer.clone(), Address::zero());

        let address = runtime
            .deploy(contract, deployer, serde_json::json!({"counter": 0}), &ctx)
            .unwrap();

        assert!(runtime.has_contract(&address));
    }

    #[test]
    fn test_call_contract() {
        let mut runtime = ContractRuntime::new().unwrap();

        let contract = ContractBuilder::new("kv")
            .function("get", vec!["key"])
            .function("set", vec!["key", "value"])
            .build()
            .unwrap();

        let deployer = Address::derive("deployer");
        let ctx = ExecutionContext::new(deployer.clone(), Address::zero());

        let address = runtime
            .deploy(contract, deployer.clone(), serde_json::json!({}), &ctx)
            .unwrap();

        // Set a value
        let mut call_ctx = ExecutionContext::new(deployer.clone(), address.clone());
        let result = runtime
            .call(
                &address,
                "set",
                &[serde_json::json!("mykey"), serde_json::json!(42)],
                &mut call_ctx,
            )
            .unwrap();

        assert_eq!(result.value, serde_json::json!(true));

        // Get the value
        let state = runtime.read_state(&address, "mykey").unwrap();
        assert_eq!(state, Some(serde_json::json!(42)));
    }

    #[test]
    fn test_gas_consumption() {
        let mut runtime = ContractRuntime::new().unwrap();

        let contract = ContractBuilder::new("test")
            .function("set", vec!["key", "value"])
            .build()
            .unwrap();

        let deployer = Address::derive("deployer");
        let ctx = ExecutionContext::new(deployer.clone(), Address::zero());

        let address = runtime
            .deploy(contract, deployer.clone(), serde_json::json!({}), &ctx)
            .unwrap();

        let mut call_ctx =
            ExecutionContext::new(deployer, address.clone()).with_gas(Gas::new(10_000));

        let result = runtime
            .call(
                &address,
                "set",
                &[serde_json::json!("key"), serde_json::json!("value")],
                &mut call_ctx,
            )
            .unwrap();

        assert!(result.gas_used > 0);
    }

    #[test]
    fn test_out_of_gas() {
        let mut runtime = ContractRuntime::new().unwrap();

        let contract = ContractBuilder::new("test")
            .function("set", vec!["key", "value"])
            .build()
            .unwrap();

        let deployer = Address::derive("deployer");
        let ctx = ExecutionContext::new(deployer.clone(), Address::zero());

        let address = runtime
            .deploy(contract, deployer.clone(), serde_json::json!({}), &ctx)
            .unwrap();

        // Very low gas limit
        let mut call_ctx = ExecutionContext::new(deployer, address.clone()).with_gas(Gas::new(10));

        let result = runtime.call(
            &address,
            "set",
            &[serde_json::json!("key"), serde_json::json!("value")],
            &mut call_ctx,
        );

        assert!(matches!(result, Err(ContractError::OutOfGas { .. })));
    }

    #[test]
    fn test_host_functions() {
        let ctx = ExecutionContext::new(Address::derive("caller"), Address::derive("contract"));

        assert_eq!(host::caller(&ctx), &Address::derive("caller"));
        assert_eq!(host::address(&ctx), &Address::derive("contract"));
        assert_eq!(host::value(&ctx), 0);

        let hash = host::hash(b"test");
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_max_depth() {
        let ctx = ExecutionContext {
            caller: Address::zero(),
            contract: Address::zero(),
            value: 0,
            gas_limit: Gas::new(1000),
            block_number: 0,
            block_timestamp: 0,
            depth: 10,
            max_depth: 10,
        };

        let result = ctx.nested();
        assert!(matches!(result, Err(ContractError::ReentrancyDetected(_))));
    }
}
