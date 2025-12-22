use ai_hash::EntryHash;
use aingle_sqlite::rusqlite::named_params;
use aingle_sqlite::rusqlite::Row;
use aingle_state::query::prelude::*;
use aingle_state::query::StateQueryError;
use aingle_types::header::WireUpdateRelationship;
use aingle_types::prelude::EntryData;
use aingle_types::prelude::HasValidationStatus;
use aingle_types::prelude::WireEntryOps;
use aingle_types::sgd_op::SgdOpType;
use aingle_zome_types::EntryType;
use aingle_zome_types::EntryVisibility;
use aingle_zome_types::Header;
use aingle_zome_types::Judged;
use aingle_zome_types::Signature;
use aingle_zome_types::SignedHeader;
use aingle_zome_types::TryFrom;
use aingle_zome_types::TryInto;
use aingle_zome_types::ValidationStatus;

#[derive(Debug, Clone)]
pub struct GetEntryOpsQuery(EntryHash);

impl GetEntryOpsQuery {
    pub fn new(hash: EntryHash) -> Self {
        Self(hash)
    }
}

// TODO: Move this to aingle types.
// TODO: This currently looks the same as
// [`WireElementOps`] but there are more things
// we can condense on entry ops due to sharing the
// same entry hash.

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct WireSgdOp {
    pub validation_status: Option<ValidationStatus>,
    pub op_type: SgdOpType,
    pub header: Header,
    pub signature: Signature,
}

impl HasValidationStatus for WireSgdOp {
    type Data = Self;

    fn validation_status(&self) -> Option<ValidationStatus> {
        self.validation_status
    }

    fn data(&self) -> &Self {
        self
    }
}

pub struct Item {
    op_type: SgdOpType,
    header: SignedHeader,
}

#[derive(Debug, Default)]
pub struct State {
    ops: WireEntryOps,
    entry_data: Option<(EntryHash, EntryType)>,
}

impl Query for GetEntryOpsQuery {
    type Item = Judged<Item>;
    type State = State;
    type Output = WireEntryOps;

    fn query(&self) -> String {
        "
        SELECT Header.blob AS header_blob, SgdOp.type AS sgd_type,
        SgdOp.validation_status AS status
        FROM SgdOp
        JOIN Header On SgdOp.header_hash = Header.hash
        WHERE SgdOp.type IN (:store_entry, :delete, :update)
        AND
        SgdOp.basis_hash = :entry_hash
        AND
        SgdOp.when_integrated IS NOT NULL
        "
        .into()
    }

    fn params(&self) -> Vec<Params> {
        let params = named_params! {
            ":store_entry": SgdOpType::StoreEntry,
            ":delete": SgdOpType::RegisterDeletedEntryHeader,
            ":update": SgdOpType::RegisterUpdatedContent,
            ":entry_hash": self.0,
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
        Ok(Default::default())
    }

    fn fold(&self, mut state: Self::State, sgd_op: Self::Item) -> StateQueryResult<Self::State> {
        match &sgd_op.data.op_type {
            SgdOpType::StoreEntry => {
                if sgd_op
                    .data
                    .header
                    .0
                    .entry_type()
                    .filter(|et| *et.visibility() == EntryVisibility::Public)
                    .is_some()
                {
                    let status = sgd_op.validation_status();
                    if state.entry_data.is_none() {
                        state.entry_data = sgd_op
                            .data
                            .header
                            .0
                            .entry_data()
                            .map(|(h, t)| (h.clone(), t.clone()));
                    }
                    state
                        .ops
                        .creates
                        .push(Judged::raw(sgd_op.data.header.try_into()?, status));
                }
            }
            SgdOpType::RegisterDeletedEntryHeader => {
                let status = sgd_op.validation_status();
                state
                    .ops
                    .deletes
                    .push(Judged::raw(sgd_op.data.header.try_into()?, status));
            }
            SgdOpType::RegisterUpdatedContent => {
                let status = sgd_op.validation_status();
                let header = sgd_op.data.header;
                state.ops.updates.push(Judged::raw(
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
        if let Some((entry_hash, entry_type)) = state.entry_data {
            let entry = stores.get_entry(&entry_hash)?;
            state.ops.entry = entry.map(|entry| EntryData { entry, entry_type });
        }
        Ok(state.ops)
    }
}
