use ai_hash::HeaderHash;
use aingle_p2p::event::GetOptions;
use aingle_sqlite::rusqlite::named_params;
use aingle_sqlite::rusqlite::Row;
use aingle_state::query::prelude::*;
use aingle_state::query::StateQueryError;
use aingle_types::element::WireElementOps;
use aingle_types::header::WireUpdateRelationship;
use aingle_types::sgd_op::SgdOpType;
use aingle_zome_types::HasValidationStatus;
use aingle_zome_types::Judged;
use aingle_zome_types::SignedHeader;
use aingle_zome_types::TryFrom;
use aingle_zome_types::TryInto;

#[derive(Debug, Clone)]
pub struct GetElementOpsQuery(HeaderHash, GetOptions);

impl GetElementOpsQuery {
    pub fn new(hash: HeaderHash, request: GetOptions) -> Self {
        Self(hash, request)
    }
}

pub struct Item {
    op_type: SgdOpType,
    header: SignedHeader,
}

impl Query for GetElementOpsQuery {
    type Item = Judged<Item>;
    type State = WireElementOps;
    type Output = Self::State;

    fn query(&self) -> String {
        let request_type = self.1.request_type.clone();
        let query = "
            SELECT Header.blob AS header_blob, SgdOp.type AS sgd_type,
            SgdOp.validation_status AS status
            FROM SgdOp
            JOIN Header On SgdOp.header_hash = Header.hash
            WHERE SgdOp.type IN (:store_element, :delete, :update)
            AND
            SgdOp.basis_hash = :header_hash
        ";
        let is_integrated = "
            AND
            SgdOp.when_integrated IS NOT NULL
        ";
        match request_type {
            aingle_p2p::event::GetRequest::All
            | aingle_p2p::event::GetRequest::Content
            | aingle_p2p::event::GetRequest::Metadata => {
                format!("{}{}", query, is_integrated)
            }
            aingle_p2p::event::GetRequest::Pending => query.into(),
        }
    }

    fn params(&self) -> Vec<Params> {
        let params = named_params! {
            ":store_element": SgdOpType::StoreElement,
            ":delete": SgdOpType::RegisterDeletedBy,
            ":update": SgdOpType::RegisterUpdatedElement,
            ":header_hash": self.0,
        };
        params.to_vec()
    }

    fn as_map(&self) -> Arc<dyn Fn(&Row) -> StateQueryResult<Self::Item>> {
        let f = |row: &Row| {
            let header = from_blob::<SignedHeader>(row.get(row.column_index("header_blob")?)?)?;
            let op_type = row.get(row.column_index("sgd_type")?)?;
            let validation_status = row.get(row.column_index("status")?)?;
            Ok(Judged::raw(Item { op_type, header }, validation_status))
        };
        Arc::new(f)
    }

    fn init_fold(&self) -> StateQueryResult<Self::State> {
        Ok(WireElementOps::new())
    }

    fn fold(&self, mut state: Self::State, sgd_op: Self::Item) -> StateQueryResult<Self::State> {
        match &sgd_op.data.op_type {
            SgdOpType::StoreElement => {
                if state.header.is_none() {
                    state.header = Some(sgd_op.map(|d| d.header));
                } else {
                    // TODO: This is weird there are multiple store elements ops for the same header??
                }
            }
            SgdOpType::RegisterDeletedBy => {
                let status = sgd_op.validation_status();
                state
                    .deletes
                    .push(Judged::raw(sgd_op.data.header.try_into()?, status));
            }
            SgdOpType::RegisterUpdatedElement => {
                let status = sgd_op.validation_status();
                let header = sgd_op.data.header;
                state.updates.push(Judged::raw(
                    WireUpdateRelationship::try_from(header)?,
                    status,
                ));
            }
            _ => return Err(StateQueryError::UnexpectedOp(sgd_op.data.op_type)),
        }
        Ok(state)
    }

    fn render<S>(&self, mut state: Self::State, stores: S) -> StateQueryResult<Self::Output>
    where
        S: Store,
    {
        let entry_hash = state.header.as_ref().and_then(|wire_op| {
            wire_op
                .data
                .0
                .entry_data()
                .map(|(hash, et)| (hash, et.visibility()))
        });
        if let Some((entry_hash, aingle_zome_types::EntryVisibility::Public)) = entry_hash {
            let entry = stores.get_entry(entry_hash)?;
            state.entry = entry;
        }
        Ok(state)
    }
}
