use crate::prelude::*;

#[cfg(feature = "mock")]
use mockall::*;

pub const ADK_NOT_REGISTERED: &str = "ADK not registered";

/// This is a cell so it can be set many times.
/// Every test needs its own mock so each test needs to set it.
use core::cell::RefCell;

#[cfg(feature = "mock")]
thread_local!(pub static ADK: RefCell<Box<dyn AdkT>> = RefCell::new(Box::new(ErrAdk)));

#[cfg(not(feature = "mock"))]
thread_local!(pub static ADK: RefCell<Box<dyn AdkT>> = RefCell::new(Box::new(HostAdk)));

/// When mocking is enabled the mockall crate automatically builds a MockAdkT for us.
/// ```ignore
/// let mut mock = MockAdkT::new();
/// mock_adk.expect_foo().times(1).etc().etc();
/// set_adk(mock_adk);
/// ```
#[cfg_attr(feature = "mock", automock)]
pub trait AdkT: Send + Sync {
    // Chain
    fn get_agent_activity(
        &self,
        get_agent_activity_input: GetAgentActivityInput,
    ) -> ExternResult<AgentActivity>;
    fn query(&self, filter: ChainQueryFilter) -> ExternResult<Vec<Element>>;
    // Ed25519
    fn sign(&self, sign: Sign) -> ExternResult<Signature>;
    fn sign_ephemeral(&self, sign_ephemeral: SignEphemeral) -> ExternResult<EphemeralSignatures>;
    fn verify_signature(&self, verify_signature: VerifySignature) -> ExternResult<bool>;
    // Entry
    fn create(&self, entry_with_def_id: EntryWithDefId) -> ExternResult<HeaderHash>;
    fn update(&self, update_input: UpdateInput) -> ExternResult<HeaderHash>;
    fn delete(&self, hash: HeaderHash) -> ExternResult<HeaderHash>;
    fn hash_entry(&self, entry: Entry) -> ExternResult<EntryHash>;
    fn get(&self, get_input: GetInput) -> ExternResult<Option<Element>>;
    fn get_details(&self, get_input: GetInput) -> ExternResult<Option<Details>>;
    // Info
    fn agent_info(&self, agent_info_input: ()) -> ExternResult<AgentInfo>;
    fn app_info(&self, app_info_input: ()) -> ExternResult<AppInfo>;
    fn saf_info(&self, saf_info_input: ()) -> ExternResult<SafInfo>;
    fn zome_info(&self, zome_info_input: ()) -> ExternResult<ZomeInfo>;
    fn call_info(&self, call_info_input: ()) -> ExternResult<CallInfo>;
    // Link
    fn create_link(&self, create_link_input: CreateLinkInput) -> ExternResult<HeaderHash>;
    fn delete_link(&self, add_link_header: HeaderHash) -> ExternResult<HeaderHash>;
    fn get_links(&self, get_links_input: GetLinksInput) -> ExternResult<Links>;
    fn get_link_details(&self, get_links_input: GetLinksInput) -> ExternResult<LinkDetails>;
    // P2P
    fn call(&self, call: Call) -> ExternResult<ZomeCallResponse>;
    fn call_remote(&self, call_remote: CallRemote) -> ExternResult<ZomeCallResponse>;
    fn emit_signal(&self, app_signal: AppSignal) -> ExternResult<()>;
    fn remote_signal(&self, remote_signal: RemoteSignal) -> ExternResult<()>;
    // Random
    fn random_bytes(&self, number_of_bytes: u32) -> ExternResult<Bytes>;
    // Time
    fn sys_time(&self, sys_time_input: ()) -> ExternResult<core::time::Duration>;
    fn schedule(&self, execute_after: std::time::Duration) -> ExternResult<()>;
    fn sleep(&self, wake_after: std::time::Duration) -> ExternResult<()>;
    // Trace
    fn trace(&self, trace_msg: TraceMsg) -> ExternResult<()>;
    // XSalsa20Poly1305
    fn create_x25519_keypair(&self, create_x25519_keypair_input: ()) -> ExternResult<X25519PubKey>;
    fn x_salsa20_poly1305_decrypt(
        &self,
        x_salsa20_poly1305_decrypt: XSalsa20Poly1305Decrypt,
    ) -> ExternResult<Option<XSalsa20Poly1305Data>>;
    fn x_salsa20_poly1305_encrypt(
        &self,
        x_salsa20_poly1305_encrypt: XSalsa20Poly1305Encrypt,
    ) -> ExternResult<XSalsa20Poly1305EncryptedData>;
    fn x_25519_x_salsa20_poly1305_encrypt(
        &self,
        x_25519_x_salsa20_poly1305_encrypt: X25519XSalsa20Poly1305Encrypt,
    ) -> ExternResult<XSalsa20Poly1305EncryptedData>;
    fn x_25519_x_salsa20_poly1305_decrypt(
        &self,
        x_25519_x_salsa20_poly1305_decrypt: X25519XSalsa20Poly1305Decrypt,
    ) -> ExternResult<Option<XSalsa20Poly1305Data>>;
    // Graph (Cortex semantic graph)
    fn graph_query(
        &self,
        input: aingle_zome_types::graph::GraphQueryInput,
    ) -> ExternResult<aingle_zome_types::graph::GraphQueryOutput>;
    fn graph_store(
        &self,
        input: aingle_zome_types::graph::GraphStoreInput,
    ) -> ExternResult<aingle_zome_types::graph::GraphStoreOutput>;
    // Memory (Titans)
    fn memory_recall(
        &self,
        input: aingle_zome_types::graph::MemoryRecallInput,
    ) -> ExternResult<aingle_zome_types::graph::MemoryRecallOutput>;
    fn memory_remember(
        &self,
        input: aingle_zome_types::graph::MemoryRememberInput,
    ) -> ExternResult<aingle_zome_types::graph::MemoryRememberOutput>;
}

/// Used as a placeholder before any other Adk is registered.
/// Generally only useful for testing but technically can be set any time.
pub struct ErrAdk;

impl ErrAdk {
    fn err<T>() -> ExternResult<T> {
        Err(WasmError::Guest(ADK_NOT_REGISTERED.to_string()))
    }
}

/// Every call is an error for the ErrAdk.
impl AdkT for ErrAdk {
    fn get_agent_activity(&self, _: GetAgentActivityInput) -> ExternResult<AgentActivity> {
        Self::err()
    }
    fn query(&self, _: ChainQueryFilter) -> ExternResult<Vec<Element>> {
        Self::err()
    }
    fn sign(&self, _: Sign) -> ExternResult<Signature> {
        Self::err()
    }
    fn sign_ephemeral(&self, _: SignEphemeral) -> ExternResult<EphemeralSignatures> {
        Self::err()
    }
    fn verify_signature(&self, _: VerifySignature) -> ExternResult<bool> {
        Self::err()
    }
    fn create(&self, _: EntryWithDefId) -> ExternResult<HeaderHash> {
        Self::err()
    }
    fn update(&self, _: UpdateInput) -> ExternResult<HeaderHash> {
        Self::err()
    }
    fn delete(&self, _: HeaderHash) -> ExternResult<HeaderHash> {
        Self::err()
    }
    fn hash_entry(&self, _: Entry) -> ExternResult<EntryHash> {
        Self::err()
    }
    fn get(&self, _: GetInput) -> ExternResult<Option<Element>> {
        Self::err()
    }
    fn get_details(&self, _: GetInput) -> ExternResult<Option<Details>> {
        Self::err()
    }
    fn agent_info(&self, _: ()) -> ExternResult<AgentInfo> {
        Self::err()
    }
    fn app_info(&self, _: ()) -> ExternResult<AppInfo> {
        Self::err()
    }
    fn saf_info(&self, _: ()) -> ExternResult<SafInfo> {
        Self::err()
    }
    fn zome_info(&self, _: ()) -> ExternResult<ZomeInfo> {
        Self::err()
    }
    fn call_info(&self, _: ()) -> ExternResult<CallInfo> {
        Self::err()
    }
    // Link
    fn create_link(&self, _: CreateLinkInput) -> ExternResult<HeaderHash> {
        Self::err()
    }
    fn delete_link(&self, _: HeaderHash) -> ExternResult<HeaderHash> {
        Self::err()
    }
    fn get_links(&self, _: GetLinksInput) -> ExternResult<Links> {
        Self::err()
    }
    fn get_link_details(&self, _: GetLinksInput) -> ExternResult<LinkDetails> {
        Self::err()
    }
    // P2P
    fn call(&self, _: Call) -> ExternResult<ZomeCallResponse> {
        Self::err()
    }
    fn call_remote(&self, _: CallRemote) -> ExternResult<ZomeCallResponse> {
        Self::err()
    }
    fn emit_signal(&self, _: AppSignal) -> ExternResult<()> {
        Self::err()
    }
    fn remote_signal(&self, _: RemoteSignal) -> ExternResult<()> {
        Self::err()
    }
    // Random
    fn random_bytes(&self, _: u32) -> ExternResult<Bytes> {
        Self::err()
    }
    // Time
    fn sys_time(&self, _: ()) -> ExternResult<core::time::Duration> {
        Self::err()
    }
    fn schedule(&self, _: std::time::Duration) -> ExternResult<()> {
        Self::err()
    }
    fn sleep(&self, _: std::time::Duration) -> ExternResult<()> {
        Self::err()
    }
    // Trace
    fn trace(&self, _: TraceMsg) -> ExternResult<()> {
        Self::err()
    }
    // XSalsa20Poly1305
    fn create_x25519_keypair(&self, _: ()) -> ExternResult<X25519PubKey> {
        Self::err()
    }
    fn x_salsa20_poly1305_decrypt(
        &self,
        _: XSalsa20Poly1305Decrypt,
    ) -> ExternResult<Option<XSalsa20Poly1305Data>> {
        Self::err()
    }
    fn x_salsa20_poly1305_encrypt(
        &self,
        _: XSalsa20Poly1305Encrypt,
    ) -> ExternResult<XSalsa20Poly1305EncryptedData> {
        Self::err()
    }
    fn x_25519_x_salsa20_poly1305_encrypt(
        &self,
        _: X25519XSalsa20Poly1305Encrypt,
    ) -> ExternResult<XSalsa20Poly1305EncryptedData> {
        Self::err()
    }
    fn x_25519_x_salsa20_poly1305_decrypt(
        &self,
        _: X25519XSalsa20Poly1305Decrypt,
    ) -> ExternResult<Option<XSalsa20Poly1305Data>> {
        Self::err()
    }
    // Graph
    fn graph_query(
        &self,
        _: aingle_zome_types::graph::GraphQueryInput,
    ) -> ExternResult<aingle_zome_types::graph::GraphQueryOutput> {
        Self::err()
    }
    fn graph_store(
        &self,
        _: aingle_zome_types::graph::GraphStoreInput,
    ) -> ExternResult<aingle_zome_types::graph::GraphStoreOutput> {
        Self::err()
    }
    // Memory
    fn memory_recall(
        &self,
        _: aingle_zome_types::graph::MemoryRecallInput,
    ) -> ExternResult<aingle_zome_types::graph::MemoryRecallOutput> {
        Self::err()
    }
    fn memory_remember(
        &self,
        _: aingle_zome_types::graph::MemoryRememberInput,
    ) -> ExternResult<aingle_zome_types::graph::MemoryRememberOutput> {
        Self::err()
    }
}

/// The ADK implemented as externs provided by the host.
pub struct HostAdk;

/// The real adk implements `host_call` for every adk function.
/// This is deferring to the standard `aingle_wasmer_guest` crate functionality.
/// Every function works exactly the same way with the same basic signatures and patterns.
/// Elsewhere in the adk are more high level wrappers around this basic trait.
#[cfg(not(feature = "mock"))]
impl AdkT for HostAdk {
    fn get_agent_activity(
        &self,
        get_agent_activity_input: GetAgentActivityInput,
    ) -> ExternResult<AgentActivity> {
        host_call::<GetAgentActivityInput, AgentActivity>(
            __get_agent_activity,
            get_agent_activity_input,
        )
    }
    fn query(&self, filter: ChainQueryFilter) -> ExternResult<Vec<Element>> {
        host_call::<ChainQueryFilter, Vec<Element>>(__query, filter)
    }

    fn sign(&self, sign: Sign) -> ExternResult<Signature> {
        host_call::<Sign, Signature>(__sign, sign)
    }
    fn sign_ephemeral(&self, sign_ephemeral: SignEphemeral) -> ExternResult<EphemeralSignatures> {
        host_call::<SignEphemeral, EphemeralSignatures>(__sign_ephemeral, sign_ephemeral)
    }
    fn verify_signature(&self, verify_signature: VerifySignature) -> ExternResult<bool> {
        host_call::<VerifySignature, bool>(__verify_signature, verify_signature)
    }

    fn create(&self, entry_with_def_id: EntryWithDefId) -> ExternResult<HeaderHash> {
        host_call::<EntryWithDefId, HeaderHash>(__create, entry_with_def_id)
    }
    fn update(&self, update_input: UpdateInput) -> ExternResult<HeaderHash> {
        host_call::<UpdateInput, HeaderHash>(__update, update_input)
    }
    fn delete(&self, hash: HeaderHash) -> ExternResult<HeaderHash> {
        host_call::<HeaderHash, HeaderHash>(__delete, hash)
    }
    fn hash_entry(&self, entry: Entry) -> ExternResult<EntryHash> {
        host_call::<Entry, EntryHash>(__hash_entry, entry)
    }
    fn get(&self, get_input: GetInput) -> ExternResult<Option<Element>> {
        host_call::<GetInput, Option<Element>>(__get, get_input)
    }
    fn get_details(&self, get_input: GetInput) -> ExternResult<Option<Details>> {
        host_call::<GetInput, Option<Details>>(__get_details, get_input)
    }

    fn agent_info(&self, _: ()) -> ExternResult<AgentInfo> {
        host_call::<(), AgentInfo>(__agent_info, ())
    }
    fn app_info(&self, _: ()) -> ExternResult<AppInfo> {
        host_call::<(), AppInfo>(__app_info, ())
    }
    fn saf_info(&self, _: ()) -> ExternResult<SafInfo> {
        host_call::<(), SafInfo>(__saf_info, ())
    }
    fn zome_info(&self, _: ()) -> ExternResult<ZomeInfo> {
        host_call::<(), ZomeInfo>(__zome_info, ())
    }
    fn call_info(&self, _: ()) -> ExternResult<CallInfo> {
        host_call::<(), CallInfo>(__call_info, ())
    }

    fn create_link(&self, create_link_input: CreateLinkInput) -> ExternResult<HeaderHash> {
        host_call::<CreateLinkInput, HeaderHash>(__create_link, create_link_input)
    }
    fn delete_link(&self, add_link_header: HeaderHash) -> ExternResult<HeaderHash> {
        host_call::<HeaderHash, HeaderHash>(__delete_link, add_link_header)
    }
    fn get_links(&self, get_links_input: GetLinksInput) -> ExternResult<Links> {
        host_call::<GetLinksInput, Links>(__get_links, get_links_input)
    }
    fn get_link_details(&self, get_links_input: GetLinksInput) -> ExternResult<LinkDetails> {
        host_call::<GetLinksInput, LinkDetails>(__get_link_details, get_links_input)
    }

    fn call(&self, call: Call) -> ExternResult<ZomeCallResponse> {
        host_call::<Call, ZomeCallResponse>(__call, call)
    }
    fn call_remote(&self, call_remote: CallRemote) -> ExternResult<ZomeCallResponse> {
        host_call::<CallRemote, ZomeCallResponse>(__call_remote, call_remote)
    }
    fn emit_signal(&self, app_signal: AppSignal) -> ExternResult<()> {
        host_call::<AppSignal, ()>(__emit_signal, app_signal)
    }
    fn remote_signal(&self, remote_signal: RemoteSignal) -> ExternResult<()> {
        host_call::<RemoteSignal, ()>(__remote_signal, remote_signal)
    }

    fn random_bytes(&self, number_of_bytes: u32) -> ExternResult<Bytes> {
        host_call::<u32, Bytes>(__random_bytes, number_of_bytes)
    }

    fn sys_time(&self, _: ()) -> ExternResult<core::time::Duration> {
        host_call::<(), core::time::Duration>(__sys_time, ())
    }
    fn schedule(&self, execute_after: std::time::Duration) -> ExternResult<()> {
        host_call::<std::time::Duration, ()>(__schedule, execute_after)
    }
    fn sleep(&self, wake_after: std::time::Duration) -> ExternResult<()> {
        host_call::<std::time::Duration, ()>(__sleep, wake_after)
    }

    fn trace(&self, trace_msg: TraceMsg) -> ExternResult<()> {
        host_call::<TraceMsg, ()>(__trace, trace_msg)
    }

    fn create_x25519_keypair(&self, _: ()) -> ExternResult<X25519PubKey> {
        host_call::<(), X25519PubKey>(__create_x25519_keypair, ())
    }
    fn x_salsa20_poly1305_decrypt(
        &self,
        x_salsa20_poly1305_decrypt: XSalsa20Poly1305Decrypt,
    ) -> ExternResult<Option<XSalsa20Poly1305Data>> {
        host_call::<XSalsa20Poly1305Decrypt, Option<XSalsa20Poly1305Data>>(
            __x_salsa20_poly1305_decrypt,
            x_salsa20_poly1305_decrypt,
        )
    }
    fn x_salsa20_poly1305_encrypt(
        &self,
        x_salsa20_poly1305_encrypt: XSalsa20Poly1305Encrypt,
    ) -> ExternResult<XSalsa20Poly1305EncryptedData> {
        host_call::<XSalsa20Poly1305Encrypt, XSalsa20Poly1305EncryptedData>(
            __x_salsa20_poly1305_encrypt,
            x_salsa20_poly1305_encrypt,
        )
    }
    fn x_25519_x_salsa20_poly1305_encrypt(
        &self,
        x_25519_x_salsa20_poly1305_encrypt: X25519XSalsa20Poly1305Encrypt,
    ) -> ExternResult<XSalsa20Poly1305EncryptedData> {
        host_call::<X25519XSalsa20Poly1305Encrypt, XSalsa20Poly1305EncryptedData>(
            __x_25519_x_salsa20_poly1305_encrypt,
            x_25519_x_salsa20_poly1305_encrypt,
        )
    }
    fn x_25519_x_salsa20_poly1305_decrypt(
        &self,
        x_25519_x_salsa20_poly1305_decrypt: X25519XSalsa20Poly1305Decrypt,
    ) -> ExternResult<Option<XSalsa20Poly1305Data>> {
        host_call::<X25519XSalsa20Poly1305Decrypt, Option<XSalsa20Poly1305Data>>(
            __x_25519_x_salsa20_poly1305_decrypt,
            x_25519_x_salsa20_poly1305_decrypt,
        )
    }

    fn graph_query(
        &self,
        input: aingle_zome_types::graph::GraphQueryInput,
    ) -> ExternResult<aingle_zome_types::graph::GraphQueryOutput> {
        host_call::<aingle_zome_types::graph::GraphQueryInput, aingle_zome_types::graph::GraphQueryOutput>(
            __graph_query,
            input,
        )
    }
    fn graph_store(
        &self,
        input: aingle_zome_types::graph::GraphStoreInput,
    ) -> ExternResult<aingle_zome_types::graph::GraphStoreOutput> {
        host_call::<aingle_zome_types::graph::GraphStoreInput, aingle_zome_types::graph::GraphStoreOutput>(
            __graph_store,
            input,
        )
    }

    fn memory_recall(
        &self,
        input: aingle_zome_types::graph::MemoryRecallInput,
    ) -> ExternResult<aingle_zome_types::graph::MemoryRecallOutput> {
        host_call::<aingle_zome_types::graph::MemoryRecallInput, aingle_zome_types::graph::MemoryRecallOutput>(
            __memory_recall,
            input,
        )
    }
    fn memory_remember(
        &self,
        input: aingle_zome_types::graph::MemoryRememberInput,
    ) -> ExternResult<aingle_zome_types::graph::MemoryRememberOutput> {
        host_call::<aingle_zome_types::graph::MemoryRememberInput, aingle_zome_types::graph::MemoryRememberOutput>(
            __memory_remember,
            input,
        )
    }
}

/// At any time the global ADK can be set to a different adk.
/// Generally this is only useful during rust unit testing.
/// When executing wasm without the `mock` feature, the host will be assumed.
pub fn set_adk<H: 'static>(adk: H)
where
    H: AdkT,
{
    ADK.with(|h| {
        *h.borrow_mut() = Box::new(adk);
    });
}
