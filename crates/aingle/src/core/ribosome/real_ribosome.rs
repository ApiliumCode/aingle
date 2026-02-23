use super::guest_callback::entry_defs::EntryDefsHostAccess;
use super::guest_callback::init::InitHostAccess;
use super::guest_callback::migrate_agent::MigrateAgentHostAccess;
use super::guest_callback::post_commit::PostCommitHostAccess;
use super::guest_callback::validate::ValidateHostAccess;
use super::guest_callback::validation_package::ValidationPackageHostAccess;
use super::host_fn::get_agent_activity::get_agent_activity;
use super::host_fn::HostFnApi;
use super::HostAccess;
use super::ZomeCallHostAccess;
use crate::core::ribosome::error::RibosomeError;
use crate::core::ribosome::error::RibosomeResult;
use crate::core::ribosome::guest_callback::entry_defs::EntryDefsInvocation;
use crate::core::ribosome::guest_callback::entry_defs::EntryDefsResult;
use crate::core::ribosome::guest_callback::genesis_self_check::GenesisSelfCheckHostAccess;
use crate::core::ribosome::guest_callback::genesis_self_check::GenesisSelfCheckInvocation;
use crate::core::ribosome::guest_callback::genesis_self_check::GenesisSelfCheckResult;
use crate::core::ribosome::guest_callback::init::InitInvocation;
use crate::core::ribosome::guest_callback::init::InitResult;
use crate::core::ribosome::guest_callback::migrate_agent::MigrateAgentInvocation;
use crate::core::ribosome::guest_callback::migrate_agent::MigrateAgentResult;
use crate::core::ribosome::guest_callback::post_commit::PostCommitInvocation;
use crate::core::ribosome::guest_callback::post_commit::PostCommitResult;
use crate::core::ribosome::guest_callback::validate::ValidateInvocation;
use crate::core::ribosome::guest_callback::validate::ValidateResult;
use crate::core::ribosome::guest_callback::validate_link::ValidateLinkHostAccess;
use crate::core::ribosome::guest_callback::validate_link::ValidateLinkInvocation;
use crate::core::ribosome::guest_callback::validate_link::ValidateLinkResult;
use crate::core::ribosome::guest_callback::validation_package::ValidationPackageInvocation;
use crate::core::ribosome::guest_callback::validation_package::ValidationPackageResult;
use crate::core::ribosome::guest_callback::CallIterator;
use crate::core::ribosome::host_fn::agent_info::agent_info;
use crate::core::ribosome::host_fn::app_info::app_info;
use crate::core::ribosome::host_fn::call::call;
use crate::core::ribosome::host_fn::call_info::call_info;
use crate::core::ribosome::host_fn::call_remote::call_remote;
use crate::core::ribosome::host_fn::capability_claims::capability_claims;
use crate::core::ribosome::host_fn::capability_grants::capability_grants;
use crate::core::ribosome::host_fn::capability_info::capability_info;
use crate::core::ribosome::host_fn::create::create;
use crate::core::ribosome::host_fn::create_link::create_link;
use crate::core::ribosome::host_fn::create_x25519_keypair::create_x25519_keypair;
use crate::core::ribosome::host_fn::delete::delete;
use crate::core::ribosome::host_fn::delete_link::delete_link;
use crate::core::ribosome::host_fn::emit_signal::emit_signal;
use crate::core::ribosome::host_fn::get::get;
use crate::core::ribosome::host_fn::get_details::get_details;
use crate::core::ribosome::host_fn::get_link_details::get_link_details;
use crate::core::ribosome::host_fn::get_links::get_links;
use crate::core::ribosome::host_fn::hash_entry::hash_entry;
use crate::core::ribosome::host_fn::query::query;
use crate::core::ribosome::host_fn::random_bytes::random_bytes;
use crate::core::ribosome::host_fn::remote_signal::remote_signal;
use crate::core::ribosome::host_fn::saf_info::saf_info;
use crate::core::ribosome::host_fn::schedule::schedule;
use crate::core::ribosome::host_fn::sign::sign;
use crate::core::ribosome::host_fn::sign_ephemeral::sign_ephemeral;
use crate::core::ribosome::host_fn::sleep::sleep;
use crate::core::ribosome::host_fn::sys_time::sys_time;
use crate::core::ribosome::host_fn::trace::trace;
use crate::core::ribosome::host_fn::unreachable::unreachable;
use crate::core::ribosome::host_fn::update::update;
use crate::core::ribosome::host_fn::verify_signature::verify_signature;
use crate::core::ribosome::host_fn::version::version;
use crate::core::ribosome::host_fn::x_25519_x_salsa20_poly1305_decrypt::x_25519_x_salsa20_poly1305_decrypt;
use crate::core::ribosome::host_fn::x_25519_x_salsa20_poly1305_encrypt::x_25519_x_salsa20_poly1305_encrypt;
use crate::core::ribosome::host_fn::x_salsa20_poly1305_decrypt::x_salsa20_poly1305_decrypt;
use crate::core::ribosome::host_fn::x_salsa20_poly1305_encrypt::x_salsa20_poly1305_encrypt;
use crate::core::ribosome::host_fn::zome_info::zome_info;
use crate::core::ribosome::host_fn::graph_query::graph_query;
use crate::core::ribosome::host_fn::graph_store::graph_store;
use crate::core::ribosome::host_fn::memory_recall::memory_recall;
use crate::core::ribosome::host_fn::memory_remember::memory_remember;
use crate::core::ribosome::CallContext;
use crate::core::ribosome::Invocation;
use crate::core::ribosome::RibosomeT;
use crate::core::ribosome::ZomeCallInvocation;
use aingle_types::prelude::*;
use fallible_iterator::FallibleIterator;

use aingle_wasmer_host::module::ModuleCache;
use aingle_wasmer_host::prelude::*;
use once_cell::sync::Lazy;
use std::sync::Arc;
use wasmer::{AsStoreMut, Function, FunctionEnv, FunctionEnvMut, Imports, Instance, Module, Store};

/// Global module cache for compiled WASM modules
static MODULE_CACHE: Lazy<ModuleCache> = Lazy::new(|| {
    // Use a filesystem cache path based on the data directory
    let cache_path = directories::ProjectDirs::from("ai", "aingle", "aingle")
        .map(|dirs| dirs.cache_dir().join("wasm_cache"));
    ModuleCache::new(cache_path)
});

/// The only RealRibosome is a Wasm ribosome.
/// note that this is cloned on every invocation so keep clones cheap!
#[derive(Clone, Debug)]
pub struct RealRibosome {
    // NOTE - Currently taking a full SafFile here.
    //      - It would be an optimization to pre-ensure the WASM bytecode
    //      - is already in the wasm cache, and only include the SafDef portion
    //      - here in the ribosome.
    pub saf_file: SafFile,
}

/// Environment data shared with host functions
#[derive(Clone)]
struct HostFnEnv {
    env: Env,
    ribosome_arc: Arc<RealRibosome>,
    context_arc: Arc<CallContext>,
}

impl RealRibosome {
    /// Create a new instance
    pub fn new(saf_file: SafFile) -> Self {
        Self { saf_file }
    }

    pub fn saf_file(&self) -> &SafFile {
        &self.saf_file
    }

    pub fn module(&self, zome_name: &ZomeName) -> RibosomeResult<Arc<Module>> {
        Ok(MODULE_CACHE.get(
            self.wasm_cache_key(zome_name)?,
            &self.saf_file.get_wasm_for_zome(zome_name)?.code(),
        )?)
    }

    pub fn wasm_cache_key(&self, zome_name: &ZomeName) -> Result<[u8; 32], SafError> {
        // TODO: make this actually the hash of the wasm once we can do that
        // watch out for cache misses in the tests that make things slooow if you change this!
        // format!("{}{}", &self.saf.saf_hash(), zome_name).into_bytes()
        let mut key = [0; 32];
        let bytes = self
            .saf_file
            .saf()
            .get_wasm_zome(zome_name)?
            .wasm_hash
            .get_raw_32();
        key.copy_from_slice(bytes);
        Ok(key)
    }

    pub fn instance(&self, call_context: CallContext) -> RibosomeResult<(Store, Instance)> {
        let zome_name = call_context.zome.zome_name().clone();
        let module = self.module(&zome_name)?;

        // IMPORTANT: Create Store from the same Engine that compiled the module.
        // In Wasmer 6.0+, modules must be instantiated with a Store using the
        // same Engine that compiled them, otherwise call_indirect fails with
        // "indirect call type mismatch".
        let mut store = Store::new(MODULE_CACHE.engine().clone());
        let (imports, func_env) = self.imports(&mut store, call_context);

        let instance = Instance::new(&mut store, &module, &imports)
            .map_err(|e| RibosomeError::WasmError(WasmError::Guest(e.to_string())))?;

        // Initialize the Env with memory, allocate, and deallocate from instance exports
        {
            let memory = instance
                .exports
                .get_memory("memory")
                .map_err(|e| {
                    RibosomeError::WasmError(WasmError::Guest(format!(
                        "Failed to get memory: {}",
                        e
                    )))
                })?
                .clone();

            let allocate = instance
                .exports
                .get_typed_function::<i32, i32>(&store, "__hc__allocate_1")
                .map_err(|e| {
                    RibosomeError::WasmError(WasmError::Guest(format!(
                        "Failed to get allocate: {}",
                        e
                    )))
                })?;

            let deallocate = instance
                .exports
                .get_typed_function::<(i32, i32), ()>(&store, "__hc__deallocate_1")
                .map_err(|e| {
                    RibosomeError::WasmError(WasmError::Guest(format!(
                        "Failed to get deallocate: {}",
                        e
                    )))
                })?;

            // Update the Env in the FunctionEnv
            let env_mut = func_env.as_mut(&mut store);
            env_mut.env.memory = Some(memory);
            env_mut.env.allocate = Some(allocate);
            env_mut.env.deallocate = Some(deallocate);
        }

        Ok((store, instance))
    }

    fn imports(
        &self,
        store: &mut Store,
        call_context: CallContext,
    ) -> (Imports, FunctionEnv<HostFnEnv>) {
        let host_fn_access = (&call_context.host_access()).into();

        let env = Env::default();
        let ribosome_arc = Arc::new((*self).clone());
        let context_arc = Arc::new(call_context);

        let host_fn_env = HostFnEnv {
            env,
            ribosome_arc: Arc::clone(&ribosome_arc),
            context_arc: Arc::clone(&context_arc),
        };
        let func_env = FunctionEnv::new(store, host_fn_env);

        let mut imports = Imports::new();

        // Helper macro to create host functions
        macro_rules! host_fn {
            ($store:expr, $func_env:expr, $host_function:expr) => {{
                let func_env_clone = $func_env.clone();
                Function::new_typed_with_env(
                    $store,
                    &func_env_clone,
                    move |mut env: FunctionEnvMut<HostFnEnv>,
                          guest_ptr: i32,
                          len: i32|
                          -> Result<i64, wasmer::RuntimeError> {
                        let (data, mut store_mut) = env.data_and_store_mut();
                        let guest_ptr: GuestPtr = guest_ptr
                            .try_into()
                            .map_err(|_| wasmer::RuntimeError::new("pointer conversion error"))?;
                        let len: Len = len
                            .try_into()
                            .map_err(|_| wasmer::RuntimeError::new("length conversion error"))?;

                        let input = data
                            .env
                            .consume_guest_input(&mut store_mut, guest_ptr, len)
                            .map_err(|e| wasmer::RuntimeError::new(format!("{:?}", e)))?;

                        let result = $host_function(
                            Arc::clone(&data.ribosome_arc),
                            Arc::clone(&data.context_arc),
                            input,
                        );

                        // WasmResult encodes success/error in bit 63, so we send:
                        // - On success: just the inner value (not wrapped in Result)
                        // - On error: the error info
                        // The guest checks is_err() on the WasmResult to determine status
                        match result {
                            Ok(output) => {
                                let ptr_len = data
                                    .env
                                    .move_data_to_guest(&mut store_mut, output)
                                    .map_err(|e| {
                                    wasmer::RuntimeError::new(format!("{:?}", e))
                                })?;
                                // Return WasmResult::ok - bit 63 is 0
                                Ok(ptr_len as i64)
                            }
                            Err(wasm_err) => {
                                // Serialize the error
                                let ptr_len = data
                                    .env
                                    .move_data_to_guest(&mut store_mut, wasm_err.to_string())
                                    .map_err(|e| wasmer::RuntimeError::new(format!("{:?}", e)))?;
                                // Return WasmResult::err - set bit 63
                                Ok((ptr_len | (1u64 << 63)) as i64)
                            }
                        }
                    },
                )
            }};
        }

        // Standard host functions
        imports.define("env", "__trace", host_fn!(store, func_env, trace));
        imports.define("env", "__hash_entry", host_fn!(store, func_env, hash_entry));
        imports.define("env", "__version", host_fn!(store, func_env, version));
        imports.define(
            "env",
            "__unreachable",
            host_fn!(store, func_env, unreachable),
        );

        if let HostFnAccess {
            keystore: Permission::Allow,
            ..
        } = host_fn_access
        {
            imports.define(
                "env",
                "__verify_signature",
                host_fn!(store, func_env, verify_signature),
            );
            imports.define("env", "__sign", host_fn!(store, func_env, sign));
            imports.define(
                "env",
                "__sign_ephemeral",
                host_fn!(store, func_env, sign_ephemeral),
            );
            imports.define(
                "env",
                "__create_x25519_keypair",
                host_fn!(store, func_env, create_x25519_keypair),
            );
            imports.define(
                "env",
                "__x_salsa20_poly1305_encrypt",
                host_fn!(store, func_env, x_salsa20_poly1305_encrypt),
            );
            imports.define(
                "env",
                "__x_salsa20_poly1305_decrypt",
                host_fn!(store, func_env, x_salsa20_poly1305_decrypt),
            );
            imports.define(
                "env",
                "__x_25519_x_salsa20_poly1305_encrypt",
                host_fn!(store, func_env, x_25519_x_salsa20_poly1305_encrypt),
            );
            imports.define(
                "env",
                "__x_25519_x_salsa20_poly1305_decrypt",
                host_fn!(store, func_env, x_25519_x_salsa20_poly1305_decrypt),
            );
        } else {
            imports.define(
                "env",
                "__verify_signature",
                host_fn!(store, func_env, unreachable),
            );
            imports.define("env", "__sign", host_fn!(store, func_env, unreachable));
            imports.define(
                "env",
                "__sign_ephemeral",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__create_x25519_keypair",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__x_salsa20_poly1305_encrypt",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__x_salsa20_poly1305_decrypt",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__x_25519_x_salsa20_poly1305_encrypt",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__x_25519_x_salsa20_poly1305_decrypt",
                host_fn!(store, func_env, unreachable),
            );
        }

        if let HostFnAccess {
            saf_bindings: Permission::Allow,
            ..
        } = host_fn_access
        {
            imports.define("env", "__zome_info", host_fn!(store, func_env, zome_info));
            imports.define("env", "__app_info", host_fn!(store, func_env, app_info));
            imports.define("env", "__saf_info", host_fn!(store, func_env, saf_info));
            imports.define("env", "__call_info", host_fn!(store, func_env, call_info));
        } else {
            imports.define("env", "__zome_info", host_fn!(store, func_env, unreachable));
            imports.define("env", "__app_info", host_fn!(store, func_env, unreachable));
            imports.define("env", "__saf_info", host_fn!(store, func_env, unreachable));
            imports.define("env", "__call_info", host_fn!(store, func_env, unreachable));
        }

        if let HostFnAccess {
            non_determinism: Permission::Allow,
            ..
        } = host_fn_access
        {
            imports.define(
                "env",
                "__random_bytes",
                host_fn!(store, func_env, random_bytes),
            );
            imports.define("env", "__sys_time", host_fn!(store, func_env, sys_time));
            imports.define("env", "__sleep", host_fn!(store, func_env, sleep));
        } else {
            imports.define(
                "env",
                "__random_bytes",
                host_fn!(store, func_env, unreachable),
            );
            imports.define("env", "__sys_time", host_fn!(store, func_env, unreachable));
            imports.define("env", "__sleep", host_fn!(store, func_env, unreachable));
        }

        if let HostFnAccess {
            agent_info: Permission::Allow,
            ..
        } = host_fn_access
        {
            imports.define("env", "__agent_info", host_fn!(store, func_env, agent_info));
            imports.define(
                "env",
                "__capability_claims",
                host_fn!(store, func_env, capability_claims),
            );
            imports.define(
                "env",
                "__capability_grants",
                host_fn!(store, func_env, capability_grants),
            );
            imports.define(
                "env",
                "__capability_info",
                host_fn!(store, func_env, capability_info),
            );
        } else {
            imports.define(
                "env",
                "__agent_info",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__capability_claims",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__capability_grants",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__capability_info",
                host_fn!(store, func_env, unreachable),
            );
        }

        if let HostFnAccess {
            read_workspace: Permission::Allow,
            ..
        } = host_fn_access
        {
            imports.define("env", "__get", host_fn!(store, func_env, get));
            imports.define(
                "env",
                "__get_details",
                host_fn!(store, func_env, get_details),
            );
            imports.define("env", "__get_links", host_fn!(store, func_env, get_links));
            imports.define(
                "env",
                "__get_link_details",
                host_fn!(store, func_env, get_link_details),
            );
            imports.define(
                "env",
                "__get_agent_activity",
                host_fn!(store, func_env, get_agent_activity),
            );
            imports.define("env", "__query", host_fn!(store, func_env, query));
        } else {
            imports.define("env", "__get", host_fn!(store, func_env, unreachable));
            imports.define(
                "env",
                "__get_details",
                host_fn!(store, func_env, unreachable),
            );
            imports.define("env", "__get_links", host_fn!(store, func_env, unreachable));
            imports.define(
                "env",
                "__get_link_details",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__get_agent_activity",
                host_fn!(store, func_env, unreachable),
            );
            imports.define("env", "__query", host_fn!(store, func_env, unreachable));
        }

        if let HostFnAccess {
            write_network: Permission::Allow,
            ..
        } = host_fn_access
        {
            imports.define(
                "env",
                "__call_remote",
                host_fn!(store, func_env, call_remote),
            );
            imports.define(
                "env",
                "__remote_signal",
                host_fn!(store, func_env, remote_signal),
            );
        } else {
            imports.define(
                "env",
                "__call_remote",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__remote_signal",
                host_fn!(store, func_env, unreachable),
            );
        }

        if let HostFnAccess {
            write_workspace: Permission::Allow,
            ..
        } = host_fn_access
        {
            imports.define("env", "__call", host_fn!(store, func_env, call));
            imports.define("env", "__create", host_fn!(store, func_env, create));
            imports.define(
                "env",
                "__emit_signal",
                host_fn!(store, func_env, emit_signal),
            );
            imports.define(
                "env",
                "__create_link",
                host_fn!(store, func_env, create_link),
            );
            imports.define(
                "env",
                "__delete_link",
                host_fn!(store, func_env, delete_link),
            );
            imports.define("env", "__update", host_fn!(store, func_env, update));
            imports.define("env", "__delete", host_fn!(store, func_env, delete));
            imports.define("env", "__schedule", host_fn!(store, func_env, schedule));
        } else {
            imports.define("env", "__call", host_fn!(store, func_env, unreachable));
            imports.define("env", "__create", host_fn!(store, func_env, unreachable));
            imports.define(
                "env",
                "__emit_signal",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__create_link",
                host_fn!(store, func_env, unreachable),
            );
            imports.define(
                "env",
                "__delete_link",
                host_fn!(store, func_env, unreachable),
            );
            imports.define("env", "__update", host_fn!(store, func_env, unreachable));
            imports.define("env", "__delete", host_fn!(store, func_env, unreachable));
            imports.define("env", "__schedule", host_fn!(store, func_env, unreachable));
        }

        // Semantic graph and Titans memory host functions.
        // These are always available (gated by Cortex connectivity, not permissions).
        imports.define(
            "env",
            "__graph_query",
            host_fn!(store, func_env, graph_query),
        );
        imports.define(
            "env",
            "__graph_store",
            host_fn!(store, func_env, graph_store),
        );
        imports.define(
            "env",
            "__memory_recall",
            host_fn!(store, func_env, memory_recall),
        );
        imports.define(
            "env",
            "__memory_remember",
            host_fn!(store, func_env, memory_remember),
        );

        (imports, func_env)
    }
}

/// General purpose macro which relies heavily on various impls of the form:
/// From<Vec<(ZomeName, $callback_result)>> for ValidationPackageResult
macro_rules! do_callback {
    ( $self:ident, $access:ident, $invocation:ident, $callback_result:ty ) => {{
        let mut results: Vec<(ZomeName, $callback_result)> = Vec::new();
        // fallible iterator syntax instead of for loop
        let mut call_iterator = $self.call_iterator($access.into(), $invocation);
        while let Some(output) = call_iterator.next()? {
            let (zome, callback_result) = output;
            let zome_name: ZomeName = zome.into();
            let callback_result: $callback_result = callback_result.into();
            // return early if we have a definitive answer, no need to keep invoking callbacks
            // if we know we are done
            if callback_result.is_definitive() {
                return Ok(vec![(zome_name, callback_result)].into());
            }
            results.push((zome_name, callback_result));
        }
        // fold all the non-definitive callbacks down into a single overall result
        Ok(results.into())
    }};
}

impl RibosomeT for RealRibosome {
    fn saf_def(&self) -> &SafDefHashed {
        self.saf_file.saf()
    }

    /// call a function in a zome for an invocation if it exists
    /// if it does not exist then return Ok(None)
    fn maybe_call<I: Invocation>(
        &self,
        host_access: HostAccess,
        invocation: &I,
        zome: &Zome,
        to_call: &FunctionName,
    ) -> Result<Option<ExternIO>, RibosomeError> {
        let call_context = CallContext {
            zome: zome.clone(),
            host_access,
        };

        match zome.zome_def() {
            ZomeDef::Wasm(_) => {
                let module = self.module(zome.zome_name())?;

                // Check if the function exists in exports
                let exports_names: Vec<String> =
                    module.exports().map(|e| e.name().to_string()).collect();

                if exports_names.iter().any(|n| n == to_call.as_ref()) {
                    // there is a callback to_call and it is implemented in the wasm
                    let (mut store, instance) = self.instance(call_context)?;

                    let mut store_mut = store.as_store_mut();
                    let input = invocation.to_owned().host_input()?;
                    let result: Result<Vec<u8>, wasmer::RuntimeError> =
                        aingle_wasmer_host::guest::call(
                            &mut store_mut,
                            Arc::new(instance),
                            to_call.as_ref(),
                            &input.0,
                        );

                    match result {
                        Ok(output_bytes) => Ok(Some(ExternIO(output_bytes))),
                        Err(e) => Err(RibosomeError::WasmError(WasmError::Guest(e.to_string()))),
                    }
                } else {
                    // the func doesn't exist
                    // the callback is not implemented
                    Ok(None)
                }
            }
            ZomeDef::Inline(zome) => {
                let input = invocation.clone().host_input()?;
                let api = HostFnApi::new(Arc::new(self.clone()), Arc::new(call_context));
                let result = zome.maybe_call(Box::new(api), to_call, input)?;
                Ok(result)
            }
        }
    }

    fn call_iterator<I: crate::core::ribosome::Invocation>(
        &self,
        access: HostAccess,
        invocation: I,
    ) -> CallIterator<Self, I> {
        CallIterator::new(access, self.clone(), invocation)
    }

    /// Runs the specified zome fn. Returns the cursor used by ADK,
    /// so that it can be passed on to source chain manager for transactional writes
    fn call_zome_function(
        &self,
        host_access: ZomeCallHostAccess,
        invocation: ZomeCallInvocation,
    ) -> RibosomeResult<ZomeCallResponse> {
        Ok(if invocation.is_authorized(&host_access)? {
            // make a copy of these for the error handling below
            let zome_name = invocation.zome.zome_name().clone();
            let fn_name = invocation.fn_name.clone();

            let guest_output: ExternIO =
                match self.call_iterator(host_access.into(), invocation).next()? {
                    Some(result) => result.1,
                    None => return Err(RibosomeError::ZomeFnNotExists(zome_name, fn_name)),
                };

            ZomeCallResponse::Ok(guest_output)
        } else {
            ZomeCallResponse::Unauthorized(
                invocation.cell_id.clone(),
                invocation.zome.zome_name().clone(),
                invocation.fn_name.clone(),
                invocation.provenance.clone(),
            )
        })
    }

    fn run_genesis_self_check(
        &self,
        access: GenesisSelfCheckHostAccess,
        invocation: GenesisSelfCheckInvocation,
    ) -> RibosomeResult<GenesisSelfCheckResult> {
        do_callback!(self, access, invocation, GenesisSelfCheckResult)
    }

    fn run_validate(
        &self,
        access: ValidateHostAccess,
        invocation: ValidateInvocation,
    ) -> RibosomeResult<ValidateResult> {
        do_callback!(self, access, invocation, ValidateCallbackResult)
    }

    fn run_validate_link<I: Invocation + 'static>(
        &self,
        access: ValidateLinkHostAccess,
        invocation: ValidateLinkInvocation<I>,
    ) -> RibosomeResult<ValidateLinkResult> {
        do_callback!(self, access, invocation, ValidateLinkCallbackResult)
    }

    fn run_init(
        &self,
        access: InitHostAccess,
        invocation: InitInvocation,
    ) -> RibosomeResult<InitResult> {
        do_callback!(self, access, invocation, InitCallbackResult)
    }

    fn run_entry_defs(
        &self,
        access: EntryDefsHostAccess,
        invocation: EntryDefsInvocation,
    ) -> RibosomeResult<EntryDefsResult> {
        do_callback!(self, access, invocation, EntryDefsCallbackResult)
    }

    fn run_migrate_agent(
        &self,
        access: MigrateAgentHostAccess,
        invocation: MigrateAgentInvocation,
    ) -> RibosomeResult<MigrateAgentResult> {
        do_callback!(self, access, invocation, MigrateAgentCallbackResult)
    }

    fn run_validation_package(
        &self,
        access: ValidationPackageHostAccess,
        invocation: ValidationPackageInvocation,
    ) -> RibosomeResult<ValidationPackageResult> {
        do_callback!(self, access, invocation, ValidationPackageCallbackResult)
    }

    fn run_post_commit(
        &self,
        access: PostCommitHostAccess,
        invocation: PostCommitInvocation,
    ) -> RibosomeResult<PostCommitResult> {
        do_callback!(self, access, invocation, PostCommitCallbackResult)
    }
}

#[cfg(test)]
#[cfg(feature = "slow_tests")]
pub mod wasm_test {
    use crate::fixt::ZomeCallHostAccessFixturator;
    use ::fixt::prelude::*;
    use adk::prelude::*;
    use aingle_state::host_fn_workspace::HostFnWorkspace;
    use aingle_wasm_test_utils::TestWasm;

    #[tokio::test(flavor = "multi_thread")]
    /// Basic checks that we can call externs internally and externally the way we want using the
    /// adk macros rather than low level rust extern syntax.
    async fn ribosome_extern_test() {
        let test_env = aingle_state::test_utils::test_cell_env();
        let test_cache = aingle_state::test_utils::test_cache_env();
        let env = test_env.env();
        let author = fake_agent_pubkey_1();
        crate::test_utils::fake_genesis(env.clone()).await.unwrap();
        let workspace = HostFnWorkspace::new(env.clone(), test_cache.env(), author)
            .await
            .unwrap();

        let mut host_access = fixt!(ZomeCallHostAccess, Predictable);
        host_access.workspace = workspace;

        let foo_result: String =
            crate::call_test_ribosome!(host_access, TestWasm::AdkExtern, "foo", ());

        assert_eq!("foo", foo_result.as_str());

        let bar_result: String =
            crate::call_test_ribosome!(host_access, TestWasm::AdkExtern, "bar", ());

        assert_eq!("foobar", bar_result.as_str());
    }
}
