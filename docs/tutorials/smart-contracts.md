# Tutorial: Smart Contracts in AIngle

This tutorial guides you through creating, deploying, and using smart contracts in AIngle.

## Prerequisites

- Rust 1.70+
- AIngle installed (`cargo install aingle`)

## 1. Create a Simple Contract

### Token Contract

```rust
use aingle_contracts::prelude::*;

fn main() -> Result<(), ContractError> {
    // Define the contract
    let contract = ContractBuilder::new("my_token")
        .version("1.0.0")
        .author("your_name")
        .description("ERC20-like token for AIngle")

        // State schema
        .state_schema(serde_json::json!({
            "name": "string",
            "symbol": "string",
            "decimals": "u8",
            "total_supply": "u64",
            "balances": "map<address, u64>",
            "allowances": "map<address, map<address, u64>>"
        }))

        // Constructor
        .constructor(vec!["name", "symbol", "initial_supply"])

        // Write functions
        .function("transfer", vec!["to", "amount"])
        .function("approve", vec!["spender", "amount"])
        .function("transfer_from", vec!["from", "to", "amount"])

        // Read functions
        .view_function("balance_of", vec!["address"], "u64")
        .view_function("allowance", vec!["owner", "spender"], "u64")
        .view_function("total_supply", vec![], "u64")

        // Special functions
        .payable_function("mint", vec!["amount"])

        .build()?;

    println!("Contract created: {:?}", contract);
    Ok(())
}
```

## 2. Implement the Logic

### File: `src/token.rs`

```rust
use aingle_contracts::prelude::*;

/// Token contract implementation
pub struct TokenContract;

impl TokenContract {
    /// Constructor - initializes the token
    pub fn constructor(
        ctx: &mut ExecutionContext,
        name: String,
        symbol: String,
        initial_supply: u64,
    ) -> Result<()> {
        let storage = ctx.storage_mut();

        storage.set("name", &name)?;
        storage.set("symbol", &symbol)?;
        storage.set("decimals", &18u8)?;
        storage.set("total_supply", &initial_supply)?;

        // Assign initial supply to deployer
        let deployer = ctx.caller();
        storage.set(&format!("balances:{}", deployer), &initial_supply)?;

        // Emit event
        ctx.emit("Transfer", serde_json::json!({
            "from": Address::zero(),
            "to": deployer,
            "amount": initial_supply
        }))?;

        Ok(())
    }

    /// Transfer tokens
    pub fn transfer(
        ctx: &mut ExecutionContext,
        to: Address,
        amount: u64,
    ) -> Result<bool> {
        let from = ctx.caller();
        Self::_transfer(ctx, from, to, amount)
    }

    /// Query balance
    pub fn balance_of(ctx: &ExecutionContext, address: Address) -> Result<u64> {
        let storage = ctx.storage();
        storage.get(&format!("balances:{}", address))
            .unwrap_or(Ok(0))
    }

    /// Internal transfer
    fn _transfer(
        ctx: &mut ExecutionContext,
        from: Address,
        to: Address,
        amount: u64,
    ) -> Result<bool> {
        let storage = ctx.storage_mut();

        // Verify balance
        let from_balance: u64 = storage
            .get(&format!("balances:{}", from))?
            .unwrap_or(0);

        if from_balance < amount {
            return Err(ContractError::InsufficientBalance);
        }

        // Update balances
        storage.set(&format!("balances:{}", from), &(from_balance - amount))?;

        let to_balance: u64 = storage
            .get(&format!("balances:{}", to))?
            .unwrap_or(0);
        storage.set(&format!("balances:{}", to), &(to_balance + amount))?;

        // Emit event
        ctx.emit("Transfer", serde_json::json!({
            "from": from,
            "to": to,
            "amount": amount
        }))?;

        Ok(true)
    }
}
```

## 3. Compile to WASM

```bash
# Compile the contract
cargo build --target wasm32-unknown-unknown --release

# The WASM file will be at:
# target/wasm32-unknown-unknown/release/my_token.wasm
```

## 4. Deploy the Contract

```rust
use aingle_contracts::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize runtime
    let runtime = ContractRuntime::new()?;

    // Load WASM
    let wasm = std::fs::read("target/wasm32-unknown-unknown/release/my_token.wasm")?;

    // Create contract from WASM
    let contract = Contract::from_wasm(&wasm)?;

    // Initial state (constructor arguments)
    let init_args = serde_json::json!({
        "name": "AIngle Token",
        "symbol": "AIG",
        "initial_supply": 1_000_000_000u64
    });

    // Deployer
    let deployer = Address::from_hex("0x1234...")?;

    // Execution context
    let mut ctx = ExecutionContext::new(deployer);

    // Deploy
    let address = runtime.deploy(contract, deployer, init_args, &mut ctx)?;

    println!("Contract deployed at: {}", address);
    Ok(())
}
```

## 5. Interact with the Contract

```rust
use aingle_contracts::prelude::*;

async fn interact_with_token() -> Result<()> {
    let runtime = ContractRuntime::new()?;
    let contract_address = Address::from_hex("0xabcd...")?;

    // Query balance (view - no gas)
    let balance: u64 = runtime.view(
        &contract_address,
        "balance_of",
        &[my_address.to_string()]
    )?;
    println!("My balance: {}", balance);

    // Transfer tokens (requires gas)
    let mut ctx = ExecutionContext::new(my_address);
    let result = runtime.call(
        &contract_address,
        "transfer",
        &[recipient.to_string(), "1000"],
        &mut ctx
    )?;

    println!("Transfer successful: {:?}", result);

    // Query events
    for event in ctx.events() {
        println!("Event: {} - {:?}", event.name, event.data);
    }

    Ok(())
}
```

## 6. Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use aingle_contracts::testing::*;

    #[test]
    fn test_transfer() {
        let mut ctx = TestContext::new();
        let alice = ctx.create_account(1000);
        let bob = ctx.create_account(0);

        // Deploy contract
        let token = ctx.deploy::<TokenContract>(serde_json::json!({
            "name": "Test Token",
            "symbol": "TST",
            "initial_supply": 1000
        }));

        // Transfer
        ctx.as_account(alice);
        let result = token.transfer(bob, 100);
        assert!(result.is_ok());

        // Verify balances
        assert_eq!(token.balance_of(alice), 900);
        assert_eq!(token.balance_of(bob), 100);
    }

    #[test]
    fn test_insufficient_balance() {
        let mut ctx = TestContext::new();
        let alice = ctx.create_account(100);
        let bob = ctx.create_account(0);

        let token = ctx.deploy::<TokenContract>(serde_json::json!({
            "name": "Test",
            "symbol": "TST",
            "initial_supply": 100
        }));

        ctx.as_account(alice);
        let result = token.transfer(bob, 200); // More than available

        assert!(matches!(result, Err(ContractError::InsufficientBalance)));
    }
}
```

## 7. Advanced Patterns

### Multi-Sig Contract

```rust
let multisig = ContractBuilder::new("multisig")
    .version("1.0.0")
    .state_schema(serde_json::json!({
        "owners": "vec<address>",
        "required_confirmations": "u8",
        "transactions": "map<u64, Transaction>",
        "confirmations": "map<u64, vec<address>>"
    }))
    .constructor(vec!["owners", "required"])
    .function("submit_transaction", vec!["to", "value", "data"])
    .function("confirm_transaction", vec!["tx_id"])
    .function("execute_transaction", vec!["tx_id"])
    .view_function("is_confirmed", vec!["tx_id"], "bool")
    .build()?;
```

### Upgradeable Contract (Proxy Pattern)

```rust
let proxy = ContractBuilder::new("proxy")
    .version("1.0.0")
    .state_schema(serde_json::json!({
        "implementation": "address",
        "admin": "address"
    }))
    .function("upgrade_to", vec!["new_implementation"])
    .function("delegate_call", vec!["data"])
    .build()?;
```

## Additional Resources

- [API Reference](../api/contracts.md)
- [Examples on GitHub](https://github.com/ApiliumCode/aingle/tree/main/examples/contracts)
- [Smart Contract Security](./smart-contracts-security.md)

---

Copyright 2019-2025 Apilium Technologies
