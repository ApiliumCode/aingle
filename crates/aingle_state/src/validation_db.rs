//! # Validation Database Types

use ai_hash::AnySgdHash;
use aingle_middleware_bytes::prelude::*;

/// The status of a [SgdOp] in limbo
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum ValidationLimboStatus {
    /// Is awaiting to be system validated
    Pending,
    /// Is waiting for dependencies so the op can proceed to system validation
    AwaitingSysDeps(AnySgdHash),
    /// Is awaiting to be app validated
    SysValidated,
    /// Is waiting for dependencies so the op can proceed to app validation
    AwaitingAppDeps(Vec<AnySgdHash>),
    /// Is awaiting to be integrated.
    AwaitingIntegration,
}
