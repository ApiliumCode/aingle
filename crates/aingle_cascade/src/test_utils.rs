use crate::authority;
use ai_hash::hash_type::AnySgd;
use ai_hash::AgentPubKey;
use ai_hash::HasHash;
use ai_hash::HeaderHash;
use aingle_p2p::actor;
use aingle_p2p::AIngleP2pCellT;
use aingle_p2p::AIngleP2pError;
use aingle_p2p::MockAIngleP2pCellT;
use aingle_sqlite::db::WriteManager;
use aingle_sqlite::prelude::DatabaseResult;
use aingle_state::mutations::insert_op;
use aingle_state::mutations::set_validation_status;
use aingle_state::mutations::set_when_integrated;
use aingle_types::activity::AgentActivityResponse;
use aingle_types::env::EnvRead;
use aingle_types::env::EnvWrite;
use aingle_types::link::WireLinkKey;
use aingle_types::link::WireLinkOps;
use aingle_types::metadata::MetadataSet;
use aingle_types::prelude::ValidationPackageResponse;
use aingle_types::sgd_op::SgdOpHashed;
use aingle_types::sgd_op::WireOps;
use aingle_types::timestamp;
use aingle_zome_types::HeaderHashed;
use aingle_zome_types::QueryFilter;
use aingle_zome_types::SignedHeader;
use aingle_zome_types::SignedHeaderHashed;
use aingle_zome_types::TryInto;
use aingle_zome_types::ValidationStatus;

pub use activity_test_data::*;
pub use element_test_data::*;
pub use entry_test_data::*;

mod activity_test_data;
mod element_test_data;
mod entry_test_data;

#[derive(Clone)]
pub struct PassThroughNetwork {
    envs: Vec<EnvRead>,
    authority: bool,
}

impl PassThroughNetwork {
    pub fn authority_for_all(envs: Vec<EnvRead>) -> Self {
        Self {
            envs,
            authority: true,
        }
    }

    pub fn authority_for_nothing(envs: Vec<EnvRead>) -> Self {
        Self {
            envs,
            authority: false,
        }
    }
}

#[derive(Clone)]
pub struct MockNetwork(std::sync::Arc<tokio::sync::Mutex<MockAIngleP2pCellT>>);

impl MockNetwork {
    pub fn new(mock: MockAIngleP2pCellT) -> Self {
        Self(std::sync::Arc::new(tokio::sync::Mutex::new(mock)))
    }
}

#[async_trait::async_trait]
impl AIngleP2pCellT for PassThroughNetwork {
    async fn get_validation_package(
        &mut self,
        _request_from: AgentPubKey,
        _header_hash: HeaderHash,
    ) -> actor::AIngleP2pResult<ValidationPackageResponse> {
        todo!()
    }

    async fn get(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetOptions,
    ) -> actor::AIngleP2pResult<Vec<WireOps>> {
        let mut out = Vec::new();
        match *sgd_hash.hash_type() {
            AnySgd::Entry => {
                for env in &self.envs {
                    let r = authority::handle_get_entry(
                        env.clone(),
                        sgd_hash.clone().into(),
                        (&options).into(),
                    )
                    .await
                    .map_err(|e| AIngleP2pError::Other(e.into()))?;
                    out.push(WireOps::Entry(r));
                }
            }
            AnySgd::Header => {
                for env in &self.envs {
                    let r = authority::handle_get_element(
                        env.clone(),
                        sgd_hash.clone().into(),
                        (&options).into(),
                    )
                    .await
                    .map_err(|e| AIngleP2pError::Other(e.into()))?;
                    out.push(WireOps::Element(r));
                }
            }
        }
        Ok(out)
    }
    async fn get_meta(
        &mut self,
        _sgd_hash: ai_hash::AnySgdHash,
        _options: actor::GetMetaOptions,
    ) -> actor::AIngleP2pResult<Vec<MetadataSet>> {
        todo!()
    }
    async fn get_links(
        &mut self,
        link_key: WireLinkKey,
        options: actor::GetLinksOptions,
    ) -> actor::AIngleP2pResult<Vec<WireLinkOps>> {
        let mut out = Vec::new();
        for env in &self.envs {
            let r = authority::handle_get_links(env.clone(), link_key.clone(), (&options).into())
                .await
                .map_err(|e| AIngleP2pError::Other(e.into()))?;
            out.push(r);
        }
        Ok(out)
    }
    async fn get_agent_activity(
        &mut self,
        agent: AgentPubKey,
        query: QueryFilter,
        options: actor::GetActivityOptions,
    ) -> actor::AIngleP2pResult<Vec<AgentActivityResponse<HeaderHash>>> {
        let mut out = Vec::new();
        for env in &self.envs {
            let r = authority::handle_get_agent_activity(
                env.clone(),
                agent.clone(),
                query.clone(),
                (&options).into(),
            )
            .await
            .map_err(|e| AIngleP2pError::Other(e.into()))?;
            out.push(r);
        }
        Ok(out)
    }

    async fn authority_for_hash(
        &mut self,
        _sgd_hash: ai_hash::AnySgdHash,
    ) -> actor::AIngleP2pResult<bool> {
        Ok(self.authority)
    }

    fn saf_hash(&self) -> ai_hash::SafHash {
        todo!()
    }

    fn from_agent(&self) -> AgentPubKey {
        todo!()
    }

    async fn join(&mut self) -> actor::AIngleP2pResult<()> {
        todo!()
    }

    async fn leave(&mut self) -> actor::AIngleP2pResult<()> {
        todo!()
    }

    async fn call_remote(
        &mut self,
        _to_agent: AgentPubKey,
        _zome_name: aingle_zome_types::ZomeName,
        _fn_name: aingle_zome_types::FunctionName,
        _cap: Option<aingle_zome_types::CapSecret>,
        _payload: aingle_zome_types::ExternIO,
    ) -> actor::AIngleP2pResult<aingle_middleware_bytes::SerializedBytes> {
        todo!()
    }

    async fn publish(
        &mut self,
        _request_validation_receipt: bool,
        _sgd_hash: ai_hash::AnySgdHash,
        _ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
        _timeout_ms: Option<u64>,
    ) -> actor::AIngleP2pResult<()> {
        todo!()
    }

    async fn send_validation_receipt(
        &mut self,
        _to_agent: AgentPubKey,
        _receipt: aingle_middleware_bytes::SerializedBytes,
    ) -> actor::AIngleP2pResult<()> {
        todo!()
    }
}

pub fn fill_db(env: &EnvWrite, op: SgdOpHashed) {
    env.conn()
        .unwrap()
        .with_commit_sync(|txn| {
            let hash = op.as_hash().clone();
            insert_op(txn, op, false).unwrap();
            set_validation_status(txn, hash.clone(), ValidationStatus::Valid).unwrap();
            set_when_integrated(txn, hash, timestamp::now()).unwrap();
            DatabaseResult::Ok(())
        })
        .unwrap();
}

pub fn fill_db_rejected(env: &EnvWrite, op: SgdOpHashed) {
    env.conn()
        .unwrap()
        .with_commit_sync(|txn| {
            let hash = op.as_hash().clone();
            insert_op(txn, op, false).unwrap();
            set_validation_status(txn, hash.clone(), ValidationStatus::Rejected).unwrap();
            set_when_integrated(txn, hash, timestamp::now()).unwrap();
            DatabaseResult::Ok(())
        })
        .unwrap();
}

pub fn fill_db_pending(env: &EnvWrite, op: SgdOpHashed) {
    env.conn()
        .unwrap()
        .with_commit_sync(|txn| {
            let hash = op.as_hash().clone();
            insert_op(txn, op, false).unwrap();
            set_validation_status(txn, hash, ValidationStatus::Valid).unwrap();
            DatabaseResult::Ok(())
        })
        .unwrap();
}

pub fn fill_db_as_author(env: &EnvWrite, op: SgdOpHashed) {
    env.conn()
        .unwrap()
        .with_commit_sync(|txn| {
            insert_op(txn, op, true).unwrap();
            DatabaseResult::Ok(())
        })
        .unwrap();
}

#[async_trait::async_trait]
impl AIngleP2pCellT for MockNetwork {
    async fn get_validation_package(
        &mut self,
        request_from: AgentPubKey,
        header_hash: HeaderHash,
    ) -> actor::AIngleP2pResult<ValidationPackageResponse> {
        self.0
            .lock()
            .await
            .get_validation_package(request_from, header_hash)
            .await
    }

    async fn get(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetOptions,
    ) -> actor::AIngleP2pResult<Vec<WireOps>> {
        self.0.lock().await.get(sgd_hash, options).await
    }

    async fn get_meta(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetMetaOptions,
    ) -> actor::AIngleP2pResult<Vec<MetadataSet>> {
        self.0.lock().await.get_meta(sgd_hash, options).await
    }

    async fn get_links(
        &mut self,
        link_key: WireLinkKey,
        options: actor::GetLinksOptions,
    ) -> actor::AIngleP2pResult<Vec<WireLinkOps>> {
        self.0.lock().await.get_links(link_key, options).await
    }

    async fn get_agent_activity(
        &mut self,
        agent: AgentPubKey,
        query: QueryFilter,
        options: actor::GetActivityOptions,
    ) -> actor::AIngleP2pResult<Vec<AgentActivityResponse<HeaderHash>>> {
        self.0
            .lock()
            .await
            .get_agent_activity(agent, query, options)
            .await
    }

    async fn authority_for_hash(
        &mut self,
        sgd_hash: ai_hash::AnySgdHash,
    ) -> actor::AIngleP2pResult<bool> {
        self.0.lock().await.authority_for_hash(sgd_hash).await
    }

    fn saf_hash(&self) -> ai_hash::SafHash {
        todo!()
    }

    fn from_agent(&self) -> AgentPubKey {
        todo!()
    }

    async fn join(&mut self) -> actor::AIngleP2pResult<()> {
        todo!()
    }

    async fn leave(&mut self) -> actor::AIngleP2pResult<()> {
        todo!()
    }

    async fn call_remote(
        &mut self,
        _to_agent: AgentPubKey,
        _zome_name: aingle_zome_types::ZomeName,
        _fn_name: aingle_zome_types::FunctionName,
        _cap: Option<aingle_zome_types::CapSecret>,
        _payload: aingle_zome_types::ExternIO,
    ) -> actor::AIngleP2pResult<aingle_middleware_bytes::SerializedBytes> {
        todo!()
    }

    async fn publish(
        &mut self,
        _request_validation_receipt: bool,
        _sgd_hash: ai_hash::AnySgdHash,
        _ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
        _timeout_ms: Option<u64>,
    ) -> actor::AIngleP2pResult<()> {
        todo!()
    }

    async fn send_validation_receipt(
        &mut self,
        _to_agent: AgentPubKey,
        _receipt: aingle_middleware_bytes::SerializedBytes,
    ) -> actor::AIngleP2pResult<()> {
        todo!()
    }
}

pub fn wire_to_shh<T: TryInto<SignedHeader> + Clone>(op: &T) -> SignedHeaderHashed {
    let r = op.clone().try_into();
    match r {
        Ok(SignedHeader(header, signature)) => {
            SignedHeaderHashed::with_presigned(HeaderHashed::from_content_sync(header), signature)
        }
        Err(_) => unreachable!(),
    }
}
