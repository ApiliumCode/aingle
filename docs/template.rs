/// # Template for Documenting Workflows

/// ### Called from (tracing context = child/follow):
///
/// ### Parameters (expected types/structures):
///
/// ### Data X (data & structure) from Source Y:
///
/// ----
///
/// ### Functions / Workflows:
///
///
/// ### Examples / Tests / Acceptance Criteria:
///
///
/// ----
///
/// ### Persisted Changes (data & structure):
///
/// ### Spawned Tasks (don't wait for result -signals/log/tracing=follow):
///
/// ### Returned Results (type & structure):
///


/// Workflow: Initiate Genesis
///
/// ### Called from (tracing context = child/follow):
/// - Conductor upon first ACTIVATION of an installed SAF (follow)
///
/// ### Parameters (expected types/structures):
/// - SAF hash (to pull from AIAI or path to file)
/// - AgentID (already registered in DeepKey)
/// - Membrane Access Payload (optional invitation code / to validate agent join)
///
/// ### Data X (data & structure) from Source Y:
/// - Get SAF from AIAI by SAF hash
/// - Get SAF from filesystem by filename
///
/// ----
/// ### Functions / Workflows:
/// - publish key (requires network to bootstrap and join space)
///
/// ### Examples / Tests / Acceptance Criteria:
///
///
/// ----
///
/// ### Persisted Changes (data & structure):
/// - all LMDB data stores created?
/// - source chain genesis entries: SAF & Author Capabilities Grant (Agent Key)
/// - SGD transforms of genesis entries in CAS
/// - bootstrapped peers from attempt to publish key and join network
///
/// ### Spawned Tasks (don't wait for result -signals/log/tracing=follow):
/// - ZomeCall:init (for processing app initialization with bridges & networking)
///
/// ### Returned Results (type & structure):
///

/// Workflow:
///
