use crate::*;
use aingle_zome_types::zome::FunctionName;

#[derive(Debug, serde::Serialize, serde::Deserialize, SerializedBytes)]
pub(crate) struct WireSgdOpData {
    pub from_agent: ai_hash::AgentPubKey,
    pub sgd_hash: ai_hash::AnySgdHash,
    pub op_data: aingle_types::sgd_op::SgdOp,
}

impl WireSgdOpData {
    pub fn encode(self) -> Result<Vec<u8>, SerializedBytesError> {
        Ok(UnsafeBytes::from(SerializedBytes::try_from(self)?).into())
    }

    pub fn decode(data: Vec<u8>) -> Result<Self, SerializedBytesError> {
        let request: SerializedBytes = UnsafeBytes::from(data).into();
        request.try_into()
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, SerializedBytes)]
#[serde(tag = "type", content = "content")]
pub(crate) enum WireMessage {
    CallRemote {
        zome_name: ZomeName,
        fn_name: FunctionName,
        cap: Option<CapSecret>,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    Publish {
        request_validation_receipt: bool,
        sgd_hash: ai_hash::AnySgdHash,
        ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
    },
    ValidationReceipt {
        #[serde(with = "serde_bytes")]
        receipt: Vec<u8>,
    },
    Get {
        sgd_hash: ai_hash::AnySgdHash,
        options: event::GetOptions,
    },
    GetMeta {
        sgd_hash: ai_hash::AnySgdHash,
        options: event::GetMetaOptions,
    },
    GetLinks {
        link_key: WireLinkKey,
        options: event::GetLinksOptions,
    },
    GetAgentActivity {
        agent: AgentPubKey,
        query: ChainQueryFilter,
        options: event::GetActivityOptions,
    },
    GetValidationPackage {
        header_hash: HeaderHash,
    },
}

impl WireMessage {
    pub fn encode(&self) -> Result<Vec<u8>, SerializedBytesError> {
        aingle_middleware_bytes::encode(&self)
    }

    pub fn decode(data: &[u8]) -> Result<Self, SerializedBytesError> {
        aingle_middleware_bytes::decode(&data)
    }

    pub fn call_remote(
        zome_name: ZomeName,
        fn_name: FunctionName,
        cap: Option<CapSecret>,
        payload: ExternIO,
    ) -> WireMessage {
        Self::CallRemote {
            zome_name,
            fn_name,
            cap,
            data: payload.into_vec(),
        }
    }

    pub fn publish(
        request_validation_receipt: bool,
        sgd_hash: ai_hash::AnySgdHash,
        ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
    ) -> WireMessage {
        Self::Publish {
            request_validation_receipt,
            sgd_hash,
            ops,
        }
    }

    pub fn validation_receipt(receipt: SerializedBytes) -> WireMessage {
        Self::ValidationReceipt {
            receipt: UnsafeBytes::from(receipt).into(),
        }
    }

    pub fn get(sgd_hash: ai_hash::AnySgdHash, options: event::GetOptions) -> WireMessage {
        Self::Get { sgd_hash, options }
    }

    pub fn get_meta(sgd_hash: ai_hash::AnySgdHash, options: event::GetMetaOptions) -> WireMessage {
        Self::GetMeta { sgd_hash, options }
    }

    pub fn get_links(link_key: WireLinkKey, options: event::GetLinksOptions) -> WireMessage {
        Self::GetLinks { link_key, options }
    }

    pub fn get_agent_activity(
        agent: AgentPubKey,
        query: ChainQueryFilter,
        options: event::GetActivityOptions,
    ) -> WireMessage {
        Self::GetAgentActivity {
            agent,
            query,
            options,
        }
    }
    pub fn get_validation_package(header_hash: HeaderHash) -> WireMessage {
        Self::GetValidationPackage { header_hash }
    }
}
