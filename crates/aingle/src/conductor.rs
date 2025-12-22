//! A Conductor manages interactions between its contained [Cell]s, as well as
//! interactions with the outside world. It is primarily a mediator of messages.
//!
//! The Conductor exposes two types of external interfaces:
//! - App interface: used by AIngle app UIs to drive the behavior of Cells,
//! - Admin interface: used to modify the Conductor itself, including adding and removing Cells
//!
//! It also exposes an internal interface to Cells themselves, allowing Cells
//! to call zome functions on other Cells, as well as to send Signals to the
//! outside world

#![deny(missing_docs)]

// TODO: clean up allows once parent is fully documented

pub mod api;
mod cell;
#[allow(clippy::module_inception)]
#[allow(missing_docs)]
pub mod conductor;
#[allow(missing_docs)]
pub mod config;
pub mod entry_def_store;
#[allow(missing_docs)]
pub mod error;
pub mod handle;
pub mod interactive;
pub mod interface;
pub mod manager;
pub mod p2p_agent_store;
pub mod p2p_metrics;
pub mod paths;
#[allow(missing_docs)]
pub mod saf_store;
pub mod state;

#[cfg(feature = "ai-integration")]
pub mod ai_service;

pub use cell::error::CellError;
pub use cell::Cell;
pub use conductor::integration_dump;
pub use conductor::Conductor;
pub use conductor::ConductorBuilder;
pub use handle::ConductorHandle;

#[cfg(feature = "ai-integration")]
pub use ai_service::create_ai_transaction;
#[cfg(feature = "ai-integration")]
pub use ai_service::AiLayerStatsSnapshot;
#[cfg(feature = "ai-integration")]
pub use ai_service::AiMetrics;
#[cfg(feature = "ai-integration")]
pub use ai_service::AiService;
#[cfg(feature = "ai-integration")]
pub use ai_service::DEFAULT_FAST_PATH_CONFIDENCE;
#[cfg(feature = "ai-integration")]
pub use ai_service::MIN_PREDICTIONS_FOR_FAST_PATH;
