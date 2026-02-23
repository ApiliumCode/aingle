//! Minimal test to debug indirect call type mismatch
//!
//! This test creates a minimal WASM module to verify if the issue
//! is with Function::new_typed_with_env or something else.

#[cfg(test)]
mod tests {
    use wasmer::{
        imports, wat2wasm, Function, FunctionEnv, FunctionEnvMut, Instance, Module, Store, Value,
    };

    /// Minimal WAT module that:
    /// 1. Imports a host function
    /// 2. Has a function table with the import
    /// 3. Calls the import via call_indirect
    const MINIMAL_WAT: &str = r#"
    (module
        ;; Import a host function with signature (i32, i32) -> i64
        (import "env" "__test_fn" (func $__test_fn (param i32 i32) (result i64)))

        ;; Function table with the imported function
        (table 2 funcref)
        (elem (i32.const 0) $__test_fn $call_test)

        ;; Type for (i32, i32) -> i64
        (type $sig_ii_l (func (param i32 i32) (result i64)))

        ;; Call the import directly
        (func (export "call_direct") (param i32 i32) (result i64)
            local.get 0
            local.get 1
            call $__test_fn
        )

        ;; Call the import via call_indirect
        (func (export "call_indirect") (param i32 i32) (result i64)
            local.get 0
            local.get 1
            i32.const 0  ;; table index of $__test_fn
            call_indirect (type $sig_ii_l)
        )

        ;; Wrapper to call via function pointer (like host_call does)
        (func $call_test (param i32 i32) (result i64)
            local.get 0
            local.get 1
            call $__test_fn
        )

        (memory (export "memory") 1)
    )
    "#;

    #[derive(Clone, Default)]
    struct TestEnv {
        call_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    #[test]
    fn test_direct_call_works() {
        let wasm_bytes = wat2wasm(MINIMAL_WAT.as_bytes()).expect("Failed to compile WAT");

        let mut store = Store::default();
        let env = TestEnv::default();
        let func_env = FunctionEnv::new(&mut store, env);

        // Create host function with environment
        let test_fn = Function::new_typed_with_env(
            &mut store,
            &func_env,
            |mut _env: FunctionEnvMut<TestEnv>, a: i32, b: i32| -> i64 {
                println!("Host function called with a={}, b={}", a, b);
                (a as i64) + (b as i64) * 1000
            },
        );

        let import_object = imports! {
            "env" => {
                "__test_fn" => test_fn,
            }
        };

        let module = Module::new(&store, wasm_bytes).expect("Failed to create module");
        let instance =
            Instance::new(&mut store, &module, &import_object).expect("Failed to instantiate");

        // Test direct call
        let call_direct = instance
            .exports
            .get_function("call_direct")
            .expect("Failed to get call_direct");
        let result = call_direct
            .call(&mut store, &[Value::I32(10), Value::I32(20)])
            .expect("Direct call failed");

        println!("Direct call result: {:?}", result);
        assert_eq!(result[0].i64(), Some(20010)); // 10 + 20*1000
    }

    #[test]
    fn test_indirect_call_with_env() {
        let wasm_bytes = wat2wasm(MINIMAL_WAT.as_bytes()).expect("Failed to compile WAT");

        let mut store = Store::default();
        let env = TestEnv::default();
        let func_env = FunctionEnv::new(&mut store, env);

        // Create host function WITH environment - this is what aingle uses
        let test_fn = Function::new_typed_with_env(
            &mut store,
            &func_env,
            |mut _env: FunctionEnvMut<TestEnv>, a: i32, b: i32| -> i64 {
                println!("Host function called with a={}, b={}", a, b);
                (a as i64) + (b as i64) * 1000
            },
        );

        let import_object = imports! {
            "env" => {
                "__test_fn" => test_fn,
            }
        };

        let module = Module::new(&store, wasm_bytes).expect("Failed to create module");
        let instance =
            Instance::new(&mut store, &module, &import_object).expect("Failed to instantiate");

        // Test indirect call - this is what fails in aingle
        let call_indirect = instance
            .exports
            .get_function("call_indirect")
            .expect("Failed to get call_indirect");

        println!("About to call indirect...");
        let result = call_indirect
            .call(&mut store, &[Value::I32(10), Value::I32(20)])
            .expect("Indirect call with env failed - THIS IS THE BUG!");

        println!("Indirect call result: {:?}", result);
        assert_eq!(result[0].i64(), Some(20010));
    }

    #[test]
    fn test_indirect_call_without_env() {
        let wasm_bytes = wat2wasm(MINIMAL_WAT.as_bytes()).expect("Failed to compile WAT");

        let mut store = Store::default();

        // Create host function WITHOUT environment
        let test_fn = Function::new_typed(&mut store, |a: i32, b: i32| -> i64 {
            println!("Host function (no env) called with a={}, b={}", a, b);
            (a as i64) + (b as i64) * 1000
        });

        let import_object = imports! {
            "env" => {
                "__test_fn" => test_fn,
            }
        };

        let module = Module::new(&store, wasm_bytes).expect("Failed to create module");
        let instance =
            Instance::new(&mut store, &module, &import_object).expect("Failed to instantiate");

        // Test indirect call without env
        let call_indirect = instance
            .exports
            .get_function("call_indirect")
            .expect("Failed to get call_indirect");

        println!("About to call indirect (no env)...");
        let result = call_indirect
            .call(&mut store, &[Value::I32(10), Value::I32(20)])
            .expect("Indirect call without env should work");

        println!("Indirect call (no env) result: {:?}", result);
        assert_eq!(result[0].i64(), Some(20010));
    }

    /// Load the actual test_wasm_zome_info.wasm and try to call __zome_info via table
    #[test]
    fn test_real_wasm_zome_info() {
        use std::fs;
        use wasmer::{Engine, Imports};

        let wasm_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../test_utils/wasm/wasm_workspace/target/wasm32-unknown-unknown/release/test_wasm_zome_info.wasm"
        );

        let wasm_bytes = fs::read(wasm_path).expect("Failed to read WASM file");
        println!("Loaded WASM file: {} bytes", wasm_bytes.len());

        let engine = Engine::default();
        let mut store = Store::new(engine);
        let env = TestEnv::default();
        let func_env = FunctionEnv::new(&mut store, env);

        let module = Module::new(&store, &wasm_bytes).expect("Failed to create module");

        // Create all required imports with the same signature (i32, i32) -> i64
        let mut imports = Imports::new();

        // Helper to create a host function
        let create_host_fn =
            |store: &mut Store, func_env: &FunctionEnv<TestEnv>, name: &'static str| {
                Function::new_typed_with_env(
                    store,
                    func_env,
                    move |mut _env: FunctionEnvMut<TestEnv>,
                          _ptr: i32,
                          _len: i32|
                          -> Result<i64, wasmer::RuntimeError> {
                        println!("Host function {} called", name);
                        // Return success result (would normally be ptr/len encoded)
                        Ok(0)
                    },
                )
            };

        // Define all imports the module needs
        let import_names = [
            "__get_agent_activity",
            "__query",
            "__sign",
            "__sign_ephemeral",
            "__verify_signature",
            "__create",
            "__update",
            "__delete",
            "__hash_entry",
            "__get",
            "__get_details",
            "__agent_info",
            "__app_info",
            "__saf_info",
            "__zome_info",
            "__call_info",
            "__create_link",
            "__delete_link",
            "__get_links",
            "__get_link_details",
            "__call",
            "__call_remote",
            "__emit_signal",
            "__remote_signal",
            "__random_bytes",
            "__sys_time",
            "__schedule",
            "__sleep",
            "__trace",
            "__create_x25519_keypair",
            "__x_salsa20_poly1305_decrypt",
            "__x_salsa20_poly1305_encrypt",
            "__x_25519_x_salsa20_poly1305_encrypt",
            "__x_25519_x_salsa20_poly1305_decrypt",
        ];

        for name in import_names {
            imports.define("env", name, create_host_fn(&mut store, &func_env, name));
        }

        println!("Attempting to instantiate module...");
        let instance = Instance::new(&mut store, &module, &imports);

        match instance {
            Ok(inst) => {
                println!("Module instantiated successfully!");

                // Try to get and call the __ai__allocate_1 function to verify basics work
                match inst.exports.get_function("__ai__allocate_1") {
                    Ok(alloc) => {
                        println!("Found __ai__allocate_1, calling...");
                        let result = alloc.call(&mut store, &[Value::I32(100)]);
                        println!("__ai__allocate_1 result: {:?}", result);
                    }
                    Err(e) => println!("Could not get __ai__allocate_1: {}", e),
                }

                // Now try to call the zome_info export if it exists
                match inst.exports.get_function("zome_info") {
                    Ok(func) => {
                        println!("Found zome_info export, attempting to call...");
                        // The function expects (ptr, len) and returns i64
                        let result = func.call(&mut store, &[Value::I32(0), Value::I32(0)]);
                        match result {
                            Ok(r) => println!("zome_info returned: {:?}", r),
                            Err(e) => println!("zome_info call FAILED: {}", e),
                        }
                    }
                    Err(e) => println!("Could not get zome_info: {}", e),
                }
            }
            Err(e) => {
                panic!("Failed to instantiate module: {}", e);
            }
        }
    }

    /// Test with multiple imports and a larger table - mirrors real aingle scenario
    #[test]
    fn test_multiple_imports_and_large_table() {
        // Create a WAT module that mirrors the real aingle scenario:
        // - Multiple imports (all with same signature)
        // - Large table with many functions
        // - Import at a high table index
        let wat = r#"
        (module
            ;; Import many host functions (like aingle does)
            (import "env" "__fn0" (func $__fn0 (param i32 i32) (result i64)))
            (import "env" "__fn1" (func $__fn1 (param i32 i32) (result i64)))
            (import "env" "__fn2" (func $__fn2 (param i32 i32) (result i64)))
            (import "env" "__fn3" (func $__fn3 (param i32 i32) (result i64)))
            (import "env" "__fn4" (func $__fn4 (param i32 i32) (result i64)))
            (import "env" "__zome_info" (func $__zome_info (param i32 i32) (result i64)))
            (import "env" "__fn6" (func $__fn6 (param i32 i32) (result i64)))
            (import "env" "__fn7" (func $__fn7 (param i32 i32) (result i64)))
            (import "env" "__fn8" (func $__fn8 (param i32 i32) (result i64)))
            (import "env" "__fn9" (func $__fn9 (param i32 i32) (result i64)))

            ;; Large table starting at offset 100
            (table 150 150 funcref)
            ;; Put dummy functions and then our imports at indices 100+
            (elem (i32.const 100)
                $__fn0 $__fn1 $__fn2 $__fn3 $__fn4
                $__zome_info $__fn6 $__fn7 $__fn8 $__fn9
                $caller $helper1 $helper2 $helper3
            )

            ;; Type for (i32, i32) -> i64
            (type $sig_ii_l (func (param i32 i32) (result i64)))

            ;; Call __zome_info via call_indirect with index 105 (100 + 5)
            (func (export "call_zome_info") (param i32 i32) (result i64)
                local.get 0
                local.get 1
                i32.const 105  ;; table index of $__zome_info
                call_indirect (type $sig_ii_l)
            )

            ;; Some helper functions to fill the table
            (func $caller (param i32 i32) (result i64)
                local.get 0
                local.get 1
                call $__zome_info
            )
            (func $helper1 (param i32 i32) (result i64) i64.const 0)
            (func $helper2 (param i32 i32) (result i64) i64.const 0)
            (func $helper3 (param i32 i32) (result i64) i64.const 0)

            (memory (export "memory") 1)
        )
        "#;

        let wasm_bytes = wat2wasm(wat.as_bytes()).expect("Failed to compile WAT");

        let mut store = Store::default();
        let env = TestEnv::default();
        let func_env = FunctionEnv::new(&mut store, env);

        // Create all host functions with environment and Result return type
        macro_rules! create_fn {
            ($name:expr) => {
                Function::new_typed_with_env(
                    &mut store,
                    &func_env,
                    |mut _env: FunctionEnvMut<TestEnv>,
                     a: i32,
                     b: i32|
                     -> Result<i64, wasmer::RuntimeError> {
                        println!("{} called with a={}, b={}", $name, a, b);
                        Ok((a as i64) + (b as i64) * 1000)
                    },
                )
            };
        }

        let import_object = imports! {
            "env" => {
                "__fn0" => create_fn!("__fn0"),
                "__fn1" => create_fn!("__fn1"),
                "__fn2" => create_fn!("__fn2"),
                "__fn3" => create_fn!("__fn3"),
                "__fn4" => create_fn!("__fn4"),
                "__zome_info" => create_fn!("__zome_info"),
                "__fn6" => create_fn!("__fn6"),
                "__fn7" => create_fn!("__fn7"),
                "__fn8" => create_fn!("__fn8"),
                "__fn9" => create_fn!("__fn9"),
            }
        };

        let module = Module::new(&store, wasm_bytes).expect("Failed to create module");
        let instance =
            Instance::new(&mut store, &module, &import_object).expect("Failed to instantiate");

        // Test call_indirect to __zome_info at index 105
        let call_zome_info = instance
            .exports
            .get_function("call_zome_info")
            .expect("Failed to get call_zome_info");

        println!("About to call zome_info via call_indirect at index 105...");
        let result = call_zome_info
            .call(&mut store, &[Value::I32(10), Value::I32(20)])
            .expect("call_indirect to __zome_info failed - THIS MIRRORS THE REAL BUG!");

        println!("call_zome_info result: {:?}", result);
        assert_eq!(result[0].i64(), Some(20010));
    }

    // Test with Result return type - this is what aingle's real_ribosome.rs uses
    #[test]
    fn test_indirect_call_with_result_return() {
        let wasm_bytes = wat2wasm(MINIMAL_WAT.as_bytes()).expect("Failed to compile WAT");

        let mut store = Store::default();
        let env = TestEnv::default();
        let func_env = FunctionEnv::new(&mut store, env);

        // Create host function WITH environment AND Result return type
        // This matches exactly what aingle's host_fn! macro does
        let test_fn = Function::new_typed_with_env(
            &mut store,
            &func_env,
            |mut _env: FunctionEnvMut<TestEnv>,
             a: i32,
             b: i32|
             -> Result<i64, wasmer::RuntimeError> {
                println!("Host function (Result return) called with a={}, b={}", a, b);
                Ok((a as i64) + (b as i64) * 1000)
            },
        );

        let import_object = imports! {
            "env" => {
                "__test_fn" => test_fn,
            }
        };

        let module = Module::new(&store, wasm_bytes).expect("Failed to create module");
        let instance =
            Instance::new(&mut store, &module, &import_object).expect("Failed to instantiate");

        // Test indirect call with Result return type
        let call_indirect = instance
            .exports
            .get_function("call_indirect")
            .expect("Failed to get call_indirect");

        println!("About to call indirect (Result return)...");
        let result = call_indirect
            .call(&mut store, &[Value::I32(10), Value::I32(20)])
            .expect("Indirect call with Result return failed - THIS IS LIKELY THE BUG!");

        println!("Indirect call (Result return) result: {:?}", result);
        assert_eq!(result[0].i64(), Some(20010));
    }
}
