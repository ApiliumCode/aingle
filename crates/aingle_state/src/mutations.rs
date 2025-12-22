use crate::entry_def::EntryDefStoreKey;
use crate::prelude::SignedValidationReceipt;
use crate::query::to_blob;
use crate::scratch::Scratch;
use crate::validation_db::ValidationLimboStatus;
use ai_hash::encode::blake2b_256;
use ai_hash::*;
use aingle_sqlite::rusqlite::named_params;
use aingle_sqlite::rusqlite::Transaction;
use aingle_types::prelude::SafDefHashed;
use aingle_types::prelude::SafWasmHashed;
use aingle_types::prelude::SgdOpError;
use aingle_types::sgd_op::OpOrder;
use aingle_types::sgd_op::SgdOpHashed;
use aingle_types::sgd_op::SgdOpLight;
use aingle_types::EntryHashed;
use aingle_zome_types::*;

pub use error::*;

mod error;

#[macro_export]
macro_rules! sql_insert {
    ($txn:expr, $table:ident, { $($field:literal : $val:expr , )+ $(,)? }) => {{
        let table = stringify!($table);
        let fielsafmes = &[ $( { $field } ,)+ ].join(",");
        let fieldvars = &[ $( { format!(":{}", $field) } ,)+ ].join(",");
        let sql = format!("INSERT INTO {} ({}) VALUES ({})", table, fielsafmes, fieldvars);
        $txn.execute(&sql, &[$(
            (format!(":{}", $field).as_str(), &$val as &dyn aingle_sqlite::rusqlite::ToSql),
        )+])
    }};
}

macro_rules! sgd_op_update {
    ($txn:expr, $hash:expr, { $($field:literal : $val:expr , )+ $(,)? }) => {{
        let fieldvars = &[ $( { format!("{} = :{}", $field, $field) } ,)+ ].join(",");
        let sql = format!(
            "
            UPDATE SgdOp 
            SET {}
            WHERE SgdOp.hash = :hash
            ", fieldvars);
        $txn.execute(&sql, &[
            (":hash", &$hash as &dyn aingle_sqlite::rusqlite::ToSql),
            $(
            (format!(":{}", $field).as_str(), &$val as &dyn aingle_sqlite::rusqlite::ToSql),
        )+])
    }};
}

/// Insert a [`SgdOp`] into the [`Scratch`].
pub fn insert_op_scratch(scratch: &mut Scratch, op: SgdOpHashed) -> StateMutationResult<()> {
    let (op, _) = op.into_inner();
    let op_light = op.to_light();
    let header = op.header();
    let signature = op.signature().clone();
    if let Some(entry) = op.entry() {
        let entry_hashed = EntryHashed::with_pre_hashed(
            entry.clone(),
            header
                .entry_hash()
                .ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?
                .clone(),
        );
        scratch.add_entry(entry_hashed);
    }
    let header_hashed = HeaderHashed::with_pre_hashed(header, op_light.header_hash().to_owned());
    let header_hashed = SignedHeaderHashed::with_presigned(header_hashed, signature);
    scratch.add_header(header_hashed);
    Ok(())
}

pub fn insert_element_scratch(scratch: &mut Scratch, element: Element) {
    let (header, entry) = element.into_inner();
    scratch.add_header(header);
    if let Some(entry) = entry.into_option() {
        scratch.add_entry(EntryHashed::from_content_sync(entry))
    }
}

/// Insert a [`SgdOp`] into the database.
pub fn insert_op(
    txn: &mut Transaction,
    op: SgdOpHashed,
    is_authored: bool,
) -> StateMutationResult<()> {
    let (op, hash) = op.into_inner();
    let op_light = op.to_light();
    let header = op.header();
    let timestamp = header.timestamp();
    let signature = op.signature().clone();
    if let Some(entry) = op.entry() {
        let entry_hashed = EntryHashed::with_pre_hashed(
            entry.clone(),
            header
                .entry_hash()
                .ok_or_else(|| SgdOpError::HeaderWithoutEntry(header.clone()))?
                .clone(),
        );
        insert_entry(txn, entry_hashed)?;
    }
    let header_hashed = HeaderHashed::with_pre_hashed(header, op_light.header_hash().to_owned());
    let header_hashed = SignedHeaderHashed::with_presigned(header_hashed, signature);
    let op_order = OpOrder::new(op_light.get_type(), header_hashed.header().timestamp());
    insert_header(txn, header_hashed)?;
    insert_op_lite(txn, op_light, hash, is_authored, op_order, timestamp)?;
    Ok(())
}

/// Insert a [`SgdOpLight`] into the database.
pub fn insert_op_lite(
    txn: &mut Transaction,
    op_lite: SgdOpLight,
    hash: SgdOpHash,
    is_authored: bool,
    order: OpOrder,
    timestamp: Timestamp,
) -> StateMutationResult<()> {
    let header_hash = op_lite.header_hash().clone();
    let basis = op_lite.sgd_basis().to_owned();
    sql_insert!(txn, SgdOp, {
        "hash": hash,
        "type": op_lite.get_type(),
        "storage_center_loc": basis.get_loc(),
        "authored_timestamp_ms": timestamp.to_sql_ms_lossy(),
        "basis_hash": basis,
        "header_hash": header_hash,
        "is_authored": is_authored,
        "require_receipt": 0,
        "blob": to_blob(op_lite)?,
        "op_order": order,
    })?;
    Ok(())
}

/// Insert a [`SignedValidationReceipt`] into the database.
pub fn insert_validation_receipt(
    txn: &mut Transaction,
    receipt: SignedValidationReceipt,
) -> StateMutationResult<()> {
    let op_hash = receipt.receipt.sgd_op_hash.clone();
    let bytes: UnsafeBytes = SerializedBytes::try_from(receipt)?.into();
    let bytes: Vec<u8> = bytes.into();
    let hash = blake2b_256(&bytes);
    sql_insert!(txn, ValidationReceipt, {
        "hash": hash,
        "op_hash": op_hash,
        "blob": bytes,
    })?;
    Ok(())
}

/// Insert a [`SafWasm`] into the database.
pub fn insert_wasm(txn: &mut Transaction, wasm: SafWasmHashed) -> StateMutationResult<()> {
    let (wasm, hash) = wasm.into_inner();
    sql_insert!(txn, Wasm, {
        "hash": hash,
        "blob": to_blob(wasm)?,
    })?;
    Ok(())
}

/// Insert a [`SafDef`] into the database.
pub fn insert_saf_def(txn: &mut Transaction, saf_def: SafDefHashed) -> StateMutationResult<()> {
    let (saf_def, hash) = saf_def.into_inner();
    sql_insert!(txn, SafDef, {
        "hash": hash,
        "blob": to_blob(saf_def)?,
    })?;
    Ok(())
}

/// Insert a [`EntryDef`] into the database.
pub fn insert_entry_def(
    txn: &mut Transaction,
    key: EntryDefStoreKey,
    entry_def: EntryDef,
) -> StateMutationResult<()> {
    sql_insert!(txn, EntryDef, {
        "key": key,
        "blob": to_blob(entry_def)?,
    })?;
    Ok(())
}

/// Insert [`ConductorState`] into the database.
pub fn insert_conductor_state(
    txn: &mut Transaction,
    bytes: SerializedBytes,
) -> StateMutationResult<()> {
    let bytes: Vec<u8> = UnsafeBytes::from(bytes).into();
    sql_insert!(txn, ConductorState, {
        "id": 1,
        "blob": bytes,
    })?;
    Ok(())
}

/// Set the validation status of a [`SgdOp`] in the database.
pub fn set_validation_status(
    txn: &mut Transaction,
    hash: SgdOpHash,
    status: ValidationStatus,
) -> StateMutationResult<()> {
    sgd_op_update!(txn, hash, {
        "validation_status": status,
    })?;
    Ok(())
}

/// Set the whether or not a receipt is required of a [`SgdOp`] in the database.
pub fn set_require_receipt(
    txn: &mut Transaction,
    hash: SgdOpHash,
    require_receipt: bool,
) -> StateMutationResult<()> {
    sgd_op_update!(txn, hash, {
        "require_receipt": require_receipt,
    })?;
    Ok(())
}

/// Set the validation stage of a [`SgdOp`] in the database.
pub fn set_validation_stage(
    txn: &mut Transaction,
    hash: SgdOpHash,
    status: ValidationLimboStatus,
) -> StateMutationResult<()> {
    let stage = match status {
        ValidationLimboStatus::Pending => None,
        ValidationLimboStatus::AwaitingSysDeps(_) => Some(0),
        ValidationLimboStatus::SysValidated => Some(1),
        ValidationLimboStatus::AwaitingAppDeps(_) => Some(2),
        ValidationLimboStatus::AwaitingIntegration => Some(3),
    };
    let now = aingle_types::timestamp::now().0;
    txn.execute(
        "
        UPDATE SgdOp
        SET
        num_validation_attempts = IFNULL(num_validation_attempts, 0) + 1,
        last_validation_attempt = :last_validation_attempt,
        validation_stage = :validation_stage
        WHERE
        SgdOp.hash = :hash
        ",
        named_params! {
            ":last_validation_attempt": now,
            ":validation_stage": stage,
            ":hash": hash,
        },
    )?;
    Ok(())
}

/// Set when a [`SgdOp`] was integrated.
pub fn set_when_integrated(
    txn: &mut Transaction,
    hash: SgdOpHash,
    time: Timestamp,
) -> StateMutationResult<()> {
    sgd_op_update!(txn, hash, {
        "when_integrated_ns": to_blob(time)?,
        "when_integrated": time,
    })?;
    Ok(())
}

/// Set when a [`SgdOp`] was last publish time
pub fn set_last_publish_time(
    txn: &mut Transaction,
    hash: SgdOpHash,
    unix_epoch: std::time::Duration,
) -> StateMutationResult<()> {
    sgd_op_update!(txn, hash, {
        "last_publish_time": unix_epoch.as_secs(),
    })?;
    Ok(())
}

/// Set the receipt count for a [`SgdOp`].
pub fn set_receipt_count(
    txn: &mut Transaction,
    hash: SgdOpHash,
    count: u32,
) -> StateMutationResult<()> {
    sgd_op_update!(txn, hash, {
        "receipt_count": count,
    })?;
    Ok(())
}

/// Add one to the receipt count for a [`SgdOp`].
pub fn add_one_receipt_count(txn: &mut Transaction, hash: &SgdOpHash) -> StateMutationResult<()> {
    txn.execute(
        "UPDATE SgdOp SET receipt_count = IFNULL(receipt_count, 0) + 1 WHERE hash = :hash;",
        named_params! { ":hash": hash },
    )?;
    Ok(())
}

/// Insert a [`Header`] into the database.
pub fn insert_header(txn: &mut Transaction, header: SignedHeaderHashed) -> StateMutationResult<()> {
    let (header, signature) = header.into_header_and_signature();
    let (header, hash) = header.into_inner();
    let header_type = header.header_type();
    let header_seq = header.header_seq();
    let author = header.author().clone();
    let prev_hash = header.prev_header().cloned();
    let private = match header.entry_type().map(|et| et.visibility()) {
        Some(EntryVisibility::Private) => true,
        Some(EntryVisibility::Public) => false,
        None => false,
    };
    match header {
        Header::CreateLink(create_link) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "base_hash": create_link.base_address,
                "zome_id": create_link.zome_id.index() as u32,
                "tag": create_link.tag,
                "blob": to_blob(SignedHeader::from((Header::CreateLink(create_link.clone()), signature)))?,
            })?;
        }
        Header::DeleteLink(delete_link) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "create_link_hash": delete_link.link_add_address,
                "blob": to_blob(SignedHeader::from((Header::DeleteLink(delete_link.clone()), signature)))?,
            })?;
        }
        Header::Create(create) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "entry_hash": create.entry_hash,
                "entry_type": create.entry_type,
                "private_entry": private,
                "blob": to_blob(SignedHeader::from((Header::Create(create.clone()), signature)))?,
            })?;
        }
        Header::Delete(delete) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "deletes_entry_hash": delete.deletes_entry_address,
                "deletes_header_hash": delete.deletes_address,
                "blob": to_blob(SignedHeader::from((Header::Delete(delete.clone()), signature)))?,
            })?;
        }
        Header::Update(update) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "entry_hash": update.entry_hash,
                "entry_type": update.entry_type,
                "original_entry_hash": update.original_entry_address,
                "original_header_hash": update.original_header_address,
                "private_entry": private,
                "blob": to_blob(SignedHeader::from((Header::Update(update.clone()), signature)))?,
            })?;
        }
        Header::InitZomesComplete(izc) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "blob": to_blob(SignedHeader::from((Header::InitZomesComplete(izc), signature)))?,
            })?;
        }
        Header::Saf(saf) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "blob": to_blob(SignedHeader::from((Header::Saf(saf), signature)))?,
            })?;
        }
        Header::AgentValidationPkg(avp) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "blob": to_blob(SignedHeader::from((Header::AgentValidationPkg(avp), signature)))?,
            })?;
        }
        Header::OpenChain(open) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "blob": to_blob(SignedHeader::from((Header::OpenChain(open), signature)))?,
            })?;
        }
        Header::CloseChain(close) => {
            sql_insert!(txn, Header, {
                "hash": hash,
                "type": header_type ,
                "seq": header_seq,
                "author": author,
                "prev_hash": prev_hash,
                "blob": to_blob(SignedHeader::from((Header::CloseChain(close), signature)))?,
            })?;
        }
    }
    Ok(())
}

/// Insert an [`Entry`] into the database.
pub fn insert_entry(txn: &mut Transaction, entry: EntryHashed) -> StateMutationResult<()> {
    let (entry, hash) = entry.into_inner();
    let mut cap_secret = None;
    let mut cap_access = None;
    let mut cap_grantor = None;
    let cap_tag = match &entry {
        Entry::CapGrant(ZomeCallCapGrant {
            tag,
            access,
            functions: _,
        }) => {
            cap_access = match access {
                CapAccess::Unrestricted => Some("unrestricted"),
                CapAccess::Transferable { secret } => {
                    cap_secret = Some(to_blob(secret)?);
                    Some("transferable")
                }
                CapAccess::Assigned {
                    secret,
                    assignees: _,
                } => {
                    cap_secret = Some(to_blob(secret)?);
                    // TODO: put assignees in when we merge in BHashSet from develop.
                    Some("assigned")
                }
            };
            // TODO: put functions in when we merge in BHashSet from develop.
            Some(tag.clone())
        }
        Entry::CapClaim(CapClaim {
            tag,
            grantor,
            secret,
        }) => {
            cap_secret = Some(to_blob(secret)?);
            cap_grantor = Some(grantor.clone());
            Some(tag.clone())
        }
        _ => None,
    };
    sql_insert!(txn, Entry, {
        "hash": hash,
        "blob": to_blob(entry)?,
        "tag": cap_tag,
        "access_type": cap_access,
        "grantor": cap_grantor,
        "cap_secret": cap_secret,
        // TODO: add cap functions and assignees
    })?;
    Ok(())
}
