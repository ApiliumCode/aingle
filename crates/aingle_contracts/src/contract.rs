//! Contract definition and DSL
//!
//! Provides a builder pattern for defining contracts.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::error::{ContractError, Result};
use crate::types::{Address, ContractId};

/// Function visibility/mutability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionType {
    /// Read-only function (view)
    View,
    /// State-mutating function
    Mutate,
    /// Payable function (can receive value)
    Payable,
    /// Internal function (not callable externally)
    Internal,
    /// Constructor (called once on deploy)
    Constructor,
}

impl Default for FunctionType {
    fn default() -> Self {
        Self::Mutate
    }
}

/// Contract function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractFunction {
    /// Function name
    pub name: String,
    /// Parameter names
    pub params: Vec<String>,
    /// Return type (as string description)
    pub returns: Option<String>,
    /// Function type
    pub function_type: FunctionType,
    /// Gas cost estimate
    pub gas_cost: u64,
    /// Documentation
    pub doc: Option<String>,
}

impl ContractFunction {
    /// Create new function
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            returns: None,
            function_type: FunctionType::default(),
            gas_cost: 1000, // Default gas cost
            doc: None,
        }
    }

    /// Add parameters
    pub fn with_params(mut self, params: Vec<&str>) -> Self {
        self.params = params.into_iter().map(String::from).collect();
        self
    }

    /// Set return type
    pub fn with_returns(mut self, returns: impl Into<String>) -> Self {
        self.returns = Some(returns.into());
        self
    }

    /// Set function type
    pub fn with_type(mut self, ft: FunctionType) -> Self {
        self.function_type = ft;
        self
    }

    /// Set gas cost
    pub fn with_gas_cost(mut self, cost: u64) -> Self {
        self.gas_cost = cost;
        self
    }

    /// Set documentation
    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    /// Check if function is read-only
    pub fn is_view(&self) -> bool {
        self.function_type == FunctionType::View
    }

    /// Check if function can receive value
    pub fn is_payable(&self) -> bool {
        self.function_type == FunctionType::Payable
    }
}

/// Contract definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    /// Contract ID
    pub id: ContractId,
    /// Contract name
    pub name: String,
    /// Version
    pub version: String,
    /// Author
    pub author: Option<String>,
    /// Description
    pub description: Option<String>,
    /// State schema (JSON Schema-like)
    pub state_schema: Option<serde_json::Value>,
    /// Contract functions
    pub functions: HashMap<String, ContractFunction>,
    /// Contract metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// WASM code (if compiled)
    pub wasm_code: Option<Vec<u8>>,
    /// Creation timestamp
    pub created_at: u64,
}

impl Contract {
    /// Get function by name
    pub fn get_function(&self, name: &str) -> Option<&ContractFunction> {
        self.functions.get(name)
    }

    /// Check if contract has function
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Get all view functions
    pub fn view_functions(&self) -> Vec<&ContractFunction> {
        self.functions.values().filter(|f| f.is_view()).collect()
    }

    /// Get all mutating functions
    pub fn mutate_functions(&self) -> Vec<&ContractFunction> {
        self.functions
            .values()
            .filter(|f| f.function_type == FunctionType::Mutate)
            .collect()
    }

    /// Compute contract hash
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.name);
        hasher.update(&self.version);
        if let Some(code) = &self.wasm_code {
            hasher.update(code);
        }
        hasher.finalize().into()
    }

    /// Check if contract has WASM code
    pub fn has_wasm(&self) -> bool {
        self.wasm_code.is_some()
    }

    /// Get contract size in bytes
    pub fn size(&self) -> usize {
        self.wasm_code.as_ref().map(|c| c.len()).unwrap_or(0)
    }
}

/// Builder for contract definitions
pub struct ContractBuilder {
    name: String,
    version: String,
    author: Option<String>,
    description: Option<String>,
    state_schema: Option<serde_json::Value>,
    functions: HashMap<String, ContractFunction>,
    metadata: HashMap<String, serde_json::Value>,
    wasm_code: Option<Vec<u8>>,
}

impl ContractBuilder {
    /// Create new contract builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: "1.0.0".to_string(),
            author: None,
            description: None,
            state_schema: None,
            functions: HashMap::new(),
            metadata: HashMap::new(),
            wasm_code: None,
        }
    }

    /// Set version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set author
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set state schema
    pub fn state_schema(mut self, schema: serde_json::Value) -> Self {
        self.state_schema = Some(schema);
        self
    }

    /// Add a function with just name and params
    pub fn function(mut self, name: &str, params: Vec<&str>) -> Self {
        let func = ContractFunction::new(name).with_params(params);
        self.functions.insert(name.to_string(), func);
        self
    }

    /// Add a view function
    pub fn view_function(mut self, name: &str, params: Vec<&str>, returns: &str) -> Self {
        let func = ContractFunction::new(name)
            .with_params(params)
            .with_returns(returns)
            .with_type(FunctionType::View);
        self.functions.insert(name.to_string(), func);
        self
    }

    /// Add a payable function
    pub fn payable_function(mut self, name: &str, params: Vec<&str>) -> Self {
        let func = ContractFunction::new(name)
            .with_params(params)
            .with_type(FunctionType::Payable);
        self.functions.insert(name.to_string(), func);
        self
    }

    /// Add constructor
    pub fn constructor(mut self, params: Vec<&str>) -> Self {
        let func = ContractFunction::new("constructor")
            .with_params(params)
            .with_type(FunctionType::Constructor);
        self.functions.insert("constructor".to_string(), func);
        self
    }

    /// Add detailed function
    pub fn add_function(mut self, func: ContractFunction) -> Self {
        self.functions.insert(func.name.clone(), func);
        self
    }

    /// Add metadata
    pub fn metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Set WASM code
    pub fn wasm(mut self, code: Vec<u8>) -> Self {
        self.wasm_code = Some(code);
        self
    }

    /// Build the contract
    pub fn build(self) -> Result<Contract> {
        if self.name.is_empty() {
            return Err(ContractError::InvalidContract(
                "Name cannot be empty".into(),
            ));
        }

        let id = if let Some(ref code) = self.wasm_code {
            ContractId::from_code(code)
        } else {
            ContractId::from_name(&self.name)
        };

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(Contract {
            id,
            name: self.name,
            version: self.version,
            author: self.author,
            description: self.description,
            state_schema: self.state_schema,
            functions: self.functions,
            metadata: self.metadata,
            wasm_code: self.wasm_code,
            created_at,
        })
    }
}

/// Contract instance (deployed contract)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractInstance {
    /// Instance address
    pub address: Address,
    /// Contract definition
    pub contract: Contract,
    /// Initial state
    pub initial_state: serde_json::Value,
    /// Deployer
    pub deployer: Address,
    /// Deploy timestamp
    pub deployed_at: u64,
}

impl ContractInstance {
    /// Create new instance
    pub fn new(contract: Contract, deployer: Address, initial_state: serde_json::Value) -> Self {
        let address = Self::compute_address(&contract, &deployer);
        let deployed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            address,
            contract,
            initial_state,
            deployer,
            deployed_at,
        }
    }

    /// Compute instance address from contract and deployer
    fn compute_address(contract: &Contract, deployer: &Address) -> Address {
        let mut hasher = Sha256::new();
        hasher.update(b"aingle_instance:");
        hasher.update(contract.id.as_bytes());
        hasher.update(deployer.as_bytes());
        hasher.update(contract.created_at.to_le_bytes());
        Address::from_bytes(hasher.finalize().into())
    }
}

/// ABI (Application Binary Interface) for contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAbi {
    /// Contract name
    pub name: String,
    /// Contract version
    pub version: String,
    /// Functions
    pub functions: Vec<AbiFunctionEntry>,
    /// Events
    pub events: Vec<AbiEventEntry>,
}

/// ABI function entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiFunctionEntry {
    /// Function name
    pub name: String,
    /// Input parameters
    pub inputs: Vec<AbiParam>,
    /// Output parameters
    pub outputs: Vec<AbiParam>,
    /// State mutability
    pub state_mutability: String,
}

/// ABI event entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiEventEntry {
    /// Event name
    pub name: String,
    /// Parameters
    pub inputs: Vec<AbiParam>,
}

/// ABI parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiParam {
    /// Parameter name
    pub name: String,
    /// Parameter type
    #[serde(rename = "type")]
    pub param_type: String,
    /// Is indexed (for events)
    #[serde(default)]
    pub indexed: bool,
}

impl Contract {
    /// Generate ABI from contract
    pub fn to_abi(&self) -> ContractAbi {
        let functions = self
            .functions
            .values()
            .map(|f| {
                let inputs = f
                    .params
                    .iter()
                    .map(|p| AbiParam {
                        name: p.clone(),
                        param_type: "any".to_string(), // Would need type info
                        indexed: false,
                    })
                    .collect();

                let outputs = if let Some(ref ret) = f.returns {
                    vec![AbiParam {
                        name: "result".to_string(),
                        param_type: ret.clone(),
                        indexed: false,
                    }]
                } else {
                    vec![]
                };

                AbiFunctionEntry {
                    name: f.name.clone(),
                    inputs,
                    outputs,
                    state_mutability: match f.function_type {
                        FunctionType::View => "view".to_string(),
                        FunctionType::Mutate => "nonpayable".to_string(),
                        FunctionType::Payable => "payable".to_string(),
                        FunctionType::Internal => "internal".to_string(),
                        FunctionType::Constructor => "constructor".to_string(),
                    },
                }
            })
            .collect();

        ContractAbi {
            name: self.name.clone(),
            version: self.version.clone(),
            functions,
            events: vec![], // Would need event definitions
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_builder() {
        let contract = ContractBuilder::new("token")
            .version("1.0.0")
            .author("AIngle")
            .description("A simple token contract")
            .state_schema(serde_json::json!({
                "balances": "map<address, u64>",
                "total_supply": "u64"
            }))
            .function("transfer", vec!["to", "amount"])
            .view_function("balance_of", vec!["address"], "u64")
            .payable_function("mint", vec!["amount"])
            .constructor(vec!["initial_supply"])
            .build()
            .unwrap();

        assert_eq!(contract.name, "token");
        assert_eq!(contract.version, "1.0.0");
        assert_eq!(contract.functions.len(), 4);
        assert!(contract.has_function("transfer"));
        assert!(contract.has_function("balance_of"));
        assert!(contract.has_function("mint"));
        assert!(contract.has_function("constructor"));
    }

    #[test]
    fn test_contract_function() {
        let func = ContractFunction::new("transfer")
            .with_params(vec!["to", "amount"])
            .with_returns("bool")
            .with_type(FunctionType::Mutate)
            .with_gas_cost(5000)
            .with_doc("Transfer tokens to another address");

        assert_eq!(func.name, "transfer");
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.gas_cost, 5000);
        assert!(!func.is_view());
    }

    #[test]
    fn test_view_function() {
        let func = ContractFunction::new("get_balance")
            .with_type(FunctionType::View)
            .with_returns("u64");

        assert!(func.is_view());
        assert!(!func.is_payable());
    }

    #[test]
    fn test_contract_instance() {
        let contract = ContractBuilder::new("test").build().unwrap();

        let deployer = Address::derive("deployer");
        let instance = ContractInstance::new(contract, deployer.clone(), serde_json::json!({}));

        assert_eq!(instance.deployer, deployer);
        assert!(!instance.address.to_hex().is_empty());
    }

    #[test]
    fn test_contract_abi() {
        let contract = ContractBuilder::new("token")
            .function("transfer", vec!["to", "amount"])
            .view_function("balance", vec!["addr"], "u64")
            .build()
            .unwrap();

        let abi = contract.to_abi();
        assert_eq!(abi.name, "token");
        assert_eq!(abi.functions.len(), 2);
    }

    #[test]
    fn test_contract_serialization() {
        let contract = ContractBuilder::new("test")
            .version("2.0.0")
            .function("foo", vec!["x"])
            .build()
            .unwrap();

        let json = serde_json::to_string(&contract).unwrap();
        let deserialized: Contract = serde_json::from_str(&json).unwrap();

        assert_eq!(contract.name, deserialized.name);
        assert_eq!(contract.version, deserialized.version);
    }
}
