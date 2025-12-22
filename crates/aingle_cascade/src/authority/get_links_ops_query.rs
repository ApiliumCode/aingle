use ai_hash::EntryHash;
use aingle_sqlite::rusqlite::named_params;
use aingle_sqlite::rusqlite::Row;
use aingle_state::query::prelude::*;
use aingle_state::query::StateQueryError;
use aingle_types::link::WireCreateLink;
use aingle_types::link::WireDeleteLink;
use aingle_types::link::WireLinkOps;
use aingle_types::sgd_op::SgdOpType;
use aingle_zome_types::HasValidationStatus;
use aingle_zome_types::Header;
use aingle_zome_types::Judged;
use aingle_zome_types::LinkTag;
use aingle_zome_types::SignedHeader;
use aingle_zome_types::ZomeId;

use super::WireLinkKey;

#[derive(Debug, Clone)]
pub struct GetLinksOpsQuery {
    base: Arc<EntryHash>,
    zome_id: ZomeId,
    tag: Option<Arc<LinkTag>>,
}

impl GetLinksOpsQuery {
    pub fn new(key: WireLinkKey) -> Self {
        Self {
            base: Arc::new(key.base),
            zome_id: key.zome_id,
            tag: key.tag.map(Arc::new),
        }
    }
}

pub struct Item {
    header: SignedHeader,
    op_type: SgdOpType,
}

impl Query for GetLinksOpsQuery {
    type Item = Judged<Item>;
    type State = WireLinkOps;
    type Output = Self::State;

    fn query(&self) -> String {
        let create = "
            SELECT Header.blob AS header_blob, SgdOp.type AS sgd_type,
            SgdOp.validation_status AS status
            FROM SgdOp
        ";
        let sub_create = "
            SELECT Header.hash FROM SgdOp
        ";
        let common = "
            JOIN Header On SgdOp.header_hash = Header.hash
            WHERE SgdOp.type = :create
            AND
            Header.base_hash = :base_hash
            AND
            Header.zome_id = :zome_id
            AND
            SgdOp.when_integrated IS NOT NULL
        ";
        // TODO: This should not be = but should be a partial match.
        let tag = "
            AND
            Header.tag = :tag
        ";
        let common_query = if self.tag.is_some() {
            format!("{}{}", common, tag)
        } else {
            common.into()
        };
        let create_query = format!("{}{}", create, common_query);
        let sub_create_query = format!("{}{}", sub_create, common_query);
        let delete_query = format!(
            "
            SELECT Header.blob AS header_blob, SgdOp.type AS sgd_type,
            SgdOp.validation_status AS status
            FROM SgdOp
            JOIN Header On SgdOp.header_hash = Header.hash
            WHERE SgdOp.type = :delete
            AND
            SgdOp.when_integrated IS NOT NULL
            AND
            Header.create_link_hash IN ({})
            ",
            sub_create_query
        );
        format!("{} UNION ALL {}", create_query, delete_query)
    }

    fn params(&self) -> Vec<Params> {
        let mut params = named_params! {
            ":create": SgdOpType::RegisterAddLink,
            ":delete": SgdOpType::RegisterRemoveLink,
            ":base_hash": self.base,
            ":zome_id": self.zome_id,
        }
        .to_vec();
        if self.tag.is_some() {
            params.extend(named_params! {
                ":tag": self.tag,
            });
        }
        params
    }

    fn as_map(&self) -> Arc<dyn Fn(&Row) -> StateQueryResult<Self::Item>> {
        let f = |row: &Row| {
            let header = from_blob::<SignedHeader>(row.get(row.column_index("header_blob")?)?)?;
            let op_type = row.get(row.column_index("sgd_type")?)?;
            let validation_status = row.get(row.column_index("status")?)?;
            Ok(Judged::raw(Item { header, op_type }, validation_status))
        };
        Arc::new(f)
    }

    fn init_fold(&self) -> StateQueryResult<Self::State> {
        Ok(WireLinkOps::new())
    }

    fn fold(&self, mut state: Self::State, sgd_op: Self::Item) -> StateQueryResult<Self::State> {
        match &sgd_op.data.op_type {
            SgdOpType::RegisterAddLink => {
                let validation_status = sgd_op.validation_status();
                let item = sgd_op.data.header;
                if let (
                    SignedHeader(Header::CreateLink(header), signature),
                    Some(validation_status),
                ) = (item, validation_status)
                {
                    if self.tag.is_some() {
                        state.creates.push(WireCreateLink::condense_base_only(
                            header,
                            signature,
                            validation_status,
                        ));
                    } else {
                        state.creates.push(WireCreateLink::condense(
                            header,
                            signature,
                            validation_status,
                        ));
                    }
                }
            }
            SgdOpType::RegisterRemoveLink => {
                let validation_status = sgd_op.validation_status();
                let item = sgd_op.data.header;
                if let (
                    SignedHeader(Header::DeleteLink(header), signature),
                    Some(validation_status),
                ) = (item, validation_status)
                {
                    state.deletes.push(WireDeleteLink::condense(
                        header,
                        signature,
                        validation_status,
                    ));
                }
            }
            _ => return Err(StateQueryError::UnexpectedOp(sgd_op.data.op_type)),
        }
        Ok(state)
    }

    fn render<S>(&self, state: Self::State, _stores: S) -> StateQueryResult<Self::Output>
    where
        S: Store,
    {
        Ok(state)
    }
}
