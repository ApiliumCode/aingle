#![deny(missing_docs)]
//! A Conductor is a dynamically changing group of [Cell]s.
//!
//! A Conductor can be managed:
//! - externally, via a [AppInterfaceApi]
//! - from within a [Cell], via [CellConductorApi]
//!
//! In normal use cases, a single AIngle user runs a single Conductor in a single process.
//! However, there's no reason we can't have multiple Conductors in a single process, simulating multiple
//! users in a testing environment.
use super::config::AdminInterfaceConfig;
use super::config::InterfaceDriver;
use super::entry_def_store::get_entry_defs;
use super::error::ConductorError;
use super::error::CreateAppError;
use super::handle::ConductorHandleImpl;
use super::interface::error::InterfaceResult;
use super::interface::websocket::spawn_admin_interface_task;
use super::interface::websocket::spawn_app_interface_task;
use super::interface::websocket::spawn_websocket_listener;
use super::interface::websocket::SIGNAL_BUFFER_SIZE;
use super::interface::SignalBroadcaster;
use super::manager::keep_alive_task;
use super::manager::spawn_task_manager;
use super::manager::ManagedTaskAdd;
use super::manager::ManagedTaskHandle;
use super::manager::TaskManagerRunHandle;
use super::p2p_agent_store::all_agent_infos;
use super::p2p_agent_store::get_single_agent_info;
use super::p2p_agent_store::inject_agent_infos;
use super::paths::EnvironmentRootPath;
use super::saf_store::RealSafStore;
use super::state::AppInterfaceId;
use super::state::ConductorState;
use super::CellError;
use super::{api::CellConductorApi, state::AppInterfaceConfig};
use super::{api::CellConductorApiT, interface::AppInterfaceRuntime};
use super::{api::RealAdminInterfaceApi, manager::TaskManagerClient};
use super::{api::RealAppInterfaceApi, p2p_agent_store};
use crate::conductor::cell::Cell;
use crate::conductor::config::ConductorConfig;
use crate::conductor::error::ConductorResult;
use crate::conductor::handle::ConductorHandle;
use crate::core::queue_consumer::InitialQueueTriggers;
use crate::{
    conductor::api::error::ConductorApiResult, core::ribosome::real_ribosome::RealRibosome,
};
use ai_hash::SafHash;
use aingle_conductor_api::AppStatusFilter;
use aingle_conductor_api::InstalledAppInfo;
use aingle_conductor_api::IntegrationStateDump;
use aingle_conductor_api::JsonDump;
use aingle_keystore::lair_keystore::spawn_lair_keystore;
use aingle_keystore::test_keystore::spawn_test_keystore;
use aingle_keystore::KeystoreSender;
use aingle_keystore::KeystoreSenderExt;
use aingle_p2p::SafHashExt;
use aingle_sqlite::db::DbKind;
use aingle_sqlite::prelude::*;
use aingle_state::mutations;
use aingle_state::prelude::from_blob;
use aingle_state::prelude::StateMutationResult;
use aingle_state::source_chain;
use aingle_types::prelude::*;
pub use builder::*;
use futures::future;
use futures::future::TryFutureExt;
use futures::stream::StreamExt;
use kitsune_p2p::agent_store::AgentInfoSigned;
use kitsune_p2p::KitsuneSpace;
use rusqlite::OptionalExtension;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::*;

#[cfg(feature = "ai-integration")]
use super::ai_service::AiService;

#[cfg(any(test, feature = "test_utils"))]
use super::handle::MockConductorHandleT;

/// Conductor-specific Cell state, this can probably be stored in a database.
/// Hypothesis: If nothing remains in this struct, then the Conductor state is
/// essentially immutable, and perhaps we just throw it out and make a new one
/// when we need to load new config, etc.
pub struct CellState {
    /// Whether or not we should call any methods on the cell
    _active: bool,
}

/// An [Cell] tracked by a Conductor, along with some [CellState]
struct CellItem<CA>
where
    CA: CellConductorApiT,
{
    cell: Arc<Cell<CA>>,
    _state: CellState,
}

pub(crate) type StopBroadcaster = tokio::sync::broadcast::Sender<()>;
pub(crate) type StopReceiver = tokio::sync::broadcast::Receiver<()>;

/// A Conductor is a group of [Cell]s
pub struct Conductor<DS = RealSafStore, CA = CellConductorApi>
where
    DS: SafStore,
    CA: CellConductorApiT,
{
    /// The collection of cells associated with this Conductor
    cells: HashMap<CellId, CellItem<CA>>,

    /// The database for persisting state related to this Conductor
    env: EnvWrite,

    /// The caches databases. These are shared across cells.
    /// There is one per unique Saf.
    caches: parking_lot::Mutex<HashMap<SafHash, EnvWrite>>,

    /// A database for storing wasm
    wasm_env: EnvWrite,

    /// The database for storing AgentInfoSigned
    p2p_env: Arc<parking_lot::Mutex<HashMap<Arc<KitsuneSpace>, EnvWrite>>>,

    p2p_metrics_env: Arc<parking_lot::Mutex<HashMap<Arc<KitsuneSpace>, EnvWrite>>>,

    /// The database for persisting [ConductorState]
    // state_db: ConductorStateDb,

    /// Set to true when `conductor.shutdown()` has been called, so that other
    /// tasks can check on the shutdown status
    shutting_down: bool,

    /// The admin websocket ports this conductor has open.
    /// This exists so that we can run tests and bind to port 0, and find out
    /// the dynamically allocated port later.
    admin_websocket_ports: Vec<u16>,

    /// Collection app interface data, keyed by id
    app_interfaces: HashMap<AppInterfaceId, AppInterfaceRuntime>,

    /// The channels and handles needed to interact with the task_manager task.
    /// If this is None, then the task manager has not yet been initialized.
    task_manager: Option<TaskManagerClient>,

    /// Placeholder for what will be the real SAF/Wasm cache
    saf_store: DS,

    /// Access to private keys for signing and encryption.
    keystore: KeystoreSender,

    /// The root environment directory where all environments are created
    root_env_dir: EnvironmentRootPath,

    /// Handle to the network actor.
    aingle_p2p: aingle_p2p::AIngleP2pRef,

    /// AI Service for intelligent validation and consensus (optional)
    #[cfg(feature = "ai-integration")]
    ai_service: Option<Arc<parking_lot::RwLock<AiService>>>,
}

impl Conductor {
    /// Create a conductor builder
    pub fn builder() -> ConductorBuilder {
        ConductorBuilder::new()
    }
}

//-----------------------------------------------------------------------------
// Public methods
//-----------------------------------------------------------------------------
impl<DS> Conductor<DS>
where
    DS: SafStore + 'static,
{
    /// Returns a port which is guaranteed to have a websocket listener with an Admin interface
    /// on it. Useful for specifying port 0 and letting the OS choose a free port.
    pub fn get_arbitrary_admin_websocket_port(&self) -> Option<u16> {
        self.admin_websocket_ports.first().copied()
    }
}

//-----------------------------------------------------------------------------
/// Methods used by the [ConductorHandle]
//-----------------------------------------------------------------------------
impl<DS> Conductor<DS>
where
    DS: SafStore + 'static,
{
    pub(super) fn cell_by_id(&self, cell_id: &CellId) -> ConductorResult<Arc<Cell>> {
        let item = self
            .cells
            .get(cell_id)
            .ok_or_else(|| ConductorError::CellMissing(cell_id.clone()))?;
        Ok(item.cell.clone())
    }

    /// A gate to put at the top of public functions to ensure that work is not
    /// attempted after a shutdown has been issued
    pub(super) fn check_running(&self) -> ConductorResult<()> {
        if self.shutting_down {
            Err(ConductorError::ShuttingDown)
        } else {
            Ok(())
        }
    }

    pub(super) fn saf_store(&self) -> &DS {
        &self.saf_store
    }

    pub(super) fn saf_store_mut(&mut self) -> &mut DS {
        &mut self.saf_store
    }

    /// Get a reference to the AI service (if enabled)
    #[cfg(feature = "ai-integration")]
    pub fn ai_service(&self) -> Option<&Arc<parking_lot::RwLock<AiService>>> {
        self.ai_service.as_ref()
    }

    /// Check if AI service is enabled and active
    #[cfg(feature = "ai-integration")]
    pub fn is_ai_enabled(&self) -> bool {
        self.ai_service
            .as_ref()
            .map(|s| s.read().is_active())
            .unwrap_or(false)
    }

    /// Broadcasts the shutdown signal to all managed tasks.
    /// To actually wait for these tasks to complete, be sure to
    /// `take_shutdown_handle` to await for completion.
    pub(super) fn shutdown(&mut self) {
        self.shutting_down = true;
        if let Some(manager) = &self.task_manager {
            tracing::info!(
                "Sending shutdown signal to {} managed tasks.",
                manager.task_stop_broadcaster().receiver_count(),
            );
            manager
                .task_stop_broadcaster()
                .send(())
                .map(|_| ())
                .unwrap_or_else(|e| {
                    error!(?e, "Couldn't broadcast stop signal to managed tasks!");
                })
        }
    }

    /// Return the handle which waits for the task manager task to complete
    pub(super) fn take_shutdown_handle(&mut self) -> Option<TaskManagerRunHandle> {
        self.task_manager
            .as_mut()
            .and_then(|manager| manager.take_handle())
    }

    /// Spawn all admin interface tasks, register them with the TaskManager,
    /// and modify the conductor accordingly, based on the config passed in
    pub(super) async fn add_admin_interfaces_via_handle(
        &mut self,
        configs: Vec<AdminInterfaceConfig>,
        handle: ConductorHandle,
    ) -> ConductorResult<()>
    where
        DS: SafStore + 'static,
    {
        let admin_api = RealAdminInterfaceApi::new(handle);
        let stop_tx = self
            .task_manager
            .as_ref()
            .expect("Task manager not started yet")
            .task_stop_broadcaster()
            .clone();

        // Closure to process each admin config item
        let spawn_from_config = |AdminInterfaceConfig { driver, .. }| {
            let admin_api = admin_api.clone();
            let stop_tx = stop_tx.clone();
            async move {
                match driver {
                    InterfaceDriver::Websocket { port } => {
                        let (listener_handle, listener) = spawn_websocket_listener(port).await?;
                        let port = listener_handle.local_addr().port().unwrap_or(port);
                        let handle: ManagedTaskHandle = spawn_admin_interface_task(
                            listener_handle,
                            listener,
                            admin_api.clone(),
                            stop_tx.subscribe(),
                        )?;
                        InterfaceResult::Ok((port, handle))
                    }
                }
            }
        };

        // spawn interface tasks, collect their JoinHandles,
        // panic on errors.
        let handles: Result<Vec<_>, _> =
            future::join_all(configs.into_iter().map(spawn_from_config))
                .await
                .into_iter()
                .collect();
        // Exit if the admin interfaces fail to be created
        let handles = handles.map_err(Box::new)?;

        {
            let mut ports = Vec::new();

            // First, register the keepalive task, to ensure the conductor doesn't shut down
            // in the absence of other "real" tasks
            self.manage_task(ManagedTaskAdd::ignore(
                tokio::spawn(keep_alive_task(stop_tx.subscribe())),
                "keepalive task",
            ))
            .await?;

            // Now that tasks are spawned, register them with the TaskManager
            for (port, handle) in handles {
                ports.push(port);
                self.manage_task(ManagedTaskAdd::ignore(
                    handle,
                    &format!("admin interface, port {}", port),
                ))
                .await?
            }
            for p in ports {
                self.add_admin_port(p);
            }
        }
        Ok(())
    }

    pub(super) async fn add_app_interface_via_handle(
        &mut self,
        port: either::Either<u16, AppInterfaceId>,
        handle: ConductorHandle,
    ) -> ConductorResult<u16> {
        let interface_id = match port {
            either::Either::Left(port) => AppInterfaceId::new(port),
            either::Either::Right(id) => id,
        };
        let port = interface_id.port();
        tracing::debug!("Attaching interface {}", port);
        let app_api = RealAppInterfaceApi::new(handle, interface_id.clone());
        // This receiver is thrown away because we can produce infinite new
        // receivers from the Sender
        let (signal_tx, _r) = tokio::sync::broadcast::channel(SIGNAL_BUFFER_SIZE);
        let stop_rx = self
            .task_manager
            .as_ref()
            .expect("Task manager not initialized")
            .task_stop_broadcaster()
            .subscribe();
        let (port, task) = spawn_app_interface_task(port, app_api, signal_tx.clone(), stop_rx)
            .await
            .map_err(Box::new)?;
        // TODO: RELIABILITY: Handle this task by restarting it if it fails and log the error
        self.manage_task(ManagedTaskAdd::ignore(
            task,
            &format!("app interface, port {}", port),
        ))
        .await?;
        let interface = AppInterfaceRuntime::Websocket { signal_tx };

        if self.app_interfaces.contains_key(&interface_id) {
            return Err(ConductorError::AppInterfaceIdCollision(interface_id));
        }

        self.app_interfaces.insert(interface_id.clone(), interface);
        let config = AppInterfaceConfig::websocket(port);
        self.update_state(|mut state| {
            state.app_interfaces.insert(interface_id, config);
            Ok(state)
        })
        .await?;
        tracing::debug!("App interface added at port: {}", port);
        Ok(port)
    }

    pub(super) async fn list_app_interfaces(&self) -> ConductorResult<Vec<u16>> {
        Ok(self
            .get_state()
            .await?
            .app_interfaces
            .values()
            .map(|config| config.driver.port())
            .collect())
    }

    pub(super) async fn register_saf_wasm(
        &self,
        saf: SafFile,
    ) -> ConductorResult<Vec<(EntryDefBufferKey, EntryDef)>> {
        let is_full_wasm_saf = saf
            .saf_def()
            .zomes
            .iter()
            .all(|(_, zome_def)| matches!(zome_def, ZomeDef::Wasm(_)));

        // Only install wasm if the SAF is composed purely of WasmZomes (no InlineZomes)
        if is_full_wasm_saf {
            Ok(self.put_wasm(saf.clone()).await?)
        } else {
            Ok(Vec::with_capacity(0))
        }
    }

    pub(super) async fn register_saf_entry_defs(
        &mut self,
        entry_defs: Vec<(EntryDefBufferKey, EntryDef)>,
    ) -> ConductorResult<()> {
        self.saf_store_mut().add_entry_defs(entry_defs);
        Ok(())
    }

    pub(super) async fn register_phenotype(&mut self, saf: SafFile) -> ConductorResult<()> {
        self.saf_store_mut().add_saf(saf);
        Ok(())
    }

    /// Start all app interfaces currently in state.
    /// This should only be run at conductor initialization.
    #[allow(irrefutable_let_patterns)]
    pub(super) async fn startup_app_interfaces_via_handle(
        &mut self,
        handle: ConductorHandle,
    ) -> ConductorResult<()> {
        for id in self.get_state().await?.app_interfaces.keys().cloned() {
            tracing::debug!("Starting up app interface: {:?}", id);
            let _ = self
                .add_app_interface_via_handle(either::Right(id), handle.clone())
                .await?;
        }
        Ok(())
    }

    pub(super) fn signal_broadcaster(&self) -> SignalBroadcaster {
        SignalBroadcaster::new(
            self.app_interfaces
                .values()
                .map(|i| i.signal_tx())
                .cloned()
                .collect(),
        )
    }

    /// Instantiate a Ribosome for use with a SAF
    pub(crate) fn get_ribosome(&self, saf_hash: &SafHash) -> ConductorResult<RealRibosome> {
        match self.saf_store().get(saf_hash) {
            Some(saf) => Ok(RealRibosome::new(saf)),
            None => Err(SafError::SafMissing(saf_hash.to_owned()).into()),
        }
    }

    /// Perform Genesis on the source chains for each of the specified CellIds.
    ///
    /// If genesis fails for any cell, this entire function fails, and all other
    /// partial or complete successes are rolled back.
    pub(super) async fn genesis_cells(
        &self,
        cell_ids_with_proofs: Vec<(CellId, Option<MembraneProof>)>,
        conductor_handle: ConductorHandle,
    ) -> ConductorResult<()> {
        let root_env_dir = std::path::PathBuf::from(self.root_env_dir.clone());

        let cells_tasks = cell_ids_with_proofs
            .into_iter()
            .map(|(cell_id, proof)| async {
                let root_env_dir = root_env_dir.clone();
                let keystore = self.keystore.clone();
                let conductor_handle = conductor_handle.clone();
                let cell_id_inner = cell_id.clone();
                let ribosome = conductor_handle
                    .get_ribosome(cell_id.saf_hash())
                    .await
                    .map_err(Box::new)?;
                tokio::spawn(async move {
                    let env = EnvWrite::open(
                        &root_env_dir,
                        DbKind::Cell(cell_id_inner.clone()),
                        keystore.clone(),
                    )?;
                    Cell::genesis(cell_id_inner, conductor_handle, env, ribosome, proof).await
                })
                .map_err(CellError::from)
                .and_then(|result| async move { result.map(|_| cell_id) })
                .await
            });
        let (success, errors): (Vec<_>, Vec<_>) = futures::future::join_all(cells_tasks)
            .await
            .into_iter()
            .partition(Result::is_ok);

        // unwrap safe because of the partition
        let success = success.into_iter().map(Result::unwrap);

        // If there were errors, cleanup and return the errors
        if !errors.is_empty() {
            for cell_id in success {
                let db = DbWrite::open(&root_env_dir, DbKind::Cell(cell_id))?;
                db.remove().await?;
            }

            // match needed to avoid Debug requirement on unwrap_err
            let errors = errors
                .into_iter()
                .map(|e| match e {
                    Err(e) => e,
                    Ok(_) => unreachable!("Safe because of the partition"),
                })
                .collect();

            Err(ConductorError::GenesisFailed { errors })
        } else {
            // No errors so return the cells
            Ok(())
        }
    }

    fn get_or_create_cache(&self, saf_hash: &SafHash) -> ConductorResult<EnvWrite> {
        let mut caches = self.caches.lock();
        match caches.get(saf_hash) {
            Some(env) => Ok(env.clone()),
            None => {
                let dir = self.root_env_dir.clone();
                let env = EnvWrite::open(
                    dir.as_ref(),
                    DbKind::Cache(saf_hash.clone()),
                    self.keystore.clone(),
                )?;
                caches.insert(saf_hash.clone(), env.clone());
                Ok(env)
            }
        }
    }

    /// Create Cells for each CellId marked active in the ConductorState db
    pub(super) async fn create_active_app_cells(
        &self,
        conductor_handle: ConductorHandle,
    ) -> ConductorResult<Vec<Result<Vec<(Cell, InitialQueueTriggers)>, CreateAppError>>> {
        // Only create the active apps
        let active_apps = self.get_state().await?.active_apps;

        // Data required to create apps
        let root_env_dir = self.root_env_dir.clone();
        let keystore = self.keystore.clone();
        let task_manager = self
            .task_manager
            .as_ref()
            .expect("Task manager not initialized");

        // Closure for creating all cells in an app
        let tasks = active_apps.into_iter().map(
            move |(installed_app_id, app): (InstalledAppId, ActiveApp)| {
                // Clone data for async block
                let root_env_dir = std::path::PathBuf::from(root_env_dir.clone());
                let conductor_handle = conductor_handle.clone();
                let keystore = keystore.clone();

                // Task that creates the cells
                async move {
                    // Only create cells not already created
                    let cells_to_create = app
                        .all_cells()
                        .filter(|cell_id| !self.cells.contains_key(cell_id))
                        .map(|cell_id| {
                            (
                                cell_id,
                                root_env_dir.clone(),
                                keystore.clone(),
                                conductor_handle.clone(),
                            )
                        });

                    use aingle_p2p::actor::AIngleP2pRefToCell;

                    // Create each cell
                    let cells_tasks = cells_to_create.map(
                        |(cell_id, dir, keystore, conductor_handle)| async move {
                            let aingle_p2p_cell = self.aingle_p2p.to_cell(
                                cell_id.saf_hash().clone(),
                                cell_id.agent_pubkey().clone(),
                            );

                            let env = EnvWrite::open(
                                &dir,
                                DbKind::Cell(cell_id.clone()),
                                keystore.clone(),
                            )?;
                            let cache = self
                                .get_or_create_cache(cell_id.saf_hash())
                                .map_err(|e| CellError::FailedToCreateCache(e.into()))?;
                            Cell::create(
                                cell_id.clone(),
                                conductor_handle.clone(),
                                env,
                                cache,
                                aingle_p2p_cell,
                                task_manager.task_add_sender().clone(),
                                task_manager.task_stop_broadcaster().clone(),
                            )
                            .await
                        },
                    );

                    // Join all the cell create tasks for this app
                    // and separate any errors
                    let (success, errors): (Vec<_>, Vec<_>) =
                        futures::future::join_all(cells_tasks)
                            .await
                            .into_iter()
                            .partition(Result::is_ok);
                    // unwrap safe because of the partition
                    let success = success.into_iter().map(Result::unwrap);

                    // If there were errors, cleanup and return the errors
                    if !errors.is_empty() {
                        for cell in success {
                            // Error needs to capture which app failed
                            cell.0.destroy().await.map_err(|e| CreateAppError::Failed {
                                installed_app_id: installed_app_id.clone(),
                                errors: vec![e],
                            })?;
                        }
                        // match needed to avoid Debug requirement on unwrap_err
                        let errors = errors
                            .into_iter()
                            .map(|e| match e {
                                Err(e) => e,
                                Ok(_) => unreachable!("Safe because of the partition"),
                            })
                            .collect();
                        Err(CreateAppError::Failed {
                            installed_app_id,
                            errors,
                        })
                    } else {
                        // No errors so return the cells
                        Ok(success.collect())
                    }
                }
            },
        );

        // Join on all apps and return a list of
        // apps that had succelly created cells
        // and any apps that encounted errors
        Ok(futures::future::join_all(tasks).await)
    }

    /// Register an app as inactive in the database
    pub(super) async fn add_inactive_app_to_db(
        &mut self,
        app: InstalledAppCommon,
    ) -> ConductorResult<InactiveApp> {
        let app = InactiveApp::new(app, DeactivationReason::NeverActivated);
        let ret = app.clone();
        self.update_state(move |mut state| {
            let is_active = state.active_apps.contains_key(app.installed_app_id());
            let is_inactive = state.inactive_apps.insert(app.clone()).is_some();
            if is_active || is_inactive {
                Err(ConductorError::AppAlreadyInstalled(
                    app.installed_app_id().clone(),
                ))
            } else {
                Ok(state)
            }
        })
        .await?;
        Ok(ret)
    }

    /// Activate an app in the database
    pub(super) async fn activate_app_in_db(
        &mut self,
        installed_app_id: InstalledAppId,
    ) -> ConductorResult<ActiveApp> {
        let (_, active_app) = self
            .update_state_prime(move |mut state| {
                let app = state
                    .inactive_apps
                    .remove(&installed_app_id)
                    .ok_or_else(|| ConductorError::AppNotInstalled(installed_app_id.clone()))?;
                let active_app = app.into_active();
                state.active_apps.insert(active_app.clone());
                Ok((state, active_app))
            })
            .await?;
        Ok(active_app)
    }

    /// Deactivate an app in the database
    pub(super) async fn deactivate_app_in_db(
        &mut self,
        installed_app_id: InstalledAppId,
        reason: DeactivationReason,
    ) -> ConductorResult<Vec<CellId>> {
        let state = self
            .update_state({
                let installed_app_id = installed_app_id.clone();
                move |mut state| {
                    let app = state
                        .active_apps
                        .remove(&installed_app_id)
                        .ok_or_else(|| ConductorError::AppNotActive(installed_app_id.clone()))?;
                    state.inactive_apps.insert(app.into_inactive(reason));
                    Ok(state)
                }
            })
            .await?;
        Ok(state
            .inactive_apps
            .get(&installed_app_id)
            .expect("This app was just put here")
            .clone()
            .all_cells()
            .cloned()
            .collect())
    }

    /// Entirely remove an app from the database
    pub(super) async fn remove_app_from_db(
        &mut self,
        installed_app_id: &InstalledAppId,
    ) -> ConductorResult<Option<Vec<CellId>>> {
        let (_state, cells_to_remove) = self
            .update_state_prime({
                let installed_app_id = installed_app_id.clone();
                move |mut state| {
                    let active = state.active_apps.remove(&installed_app_id);
                    let inactive = state.inactive_apps.remove(&installed_app_id);
                    let cells = active
                        .map(|a| a.into_common())
                        .or_else(|| inactive.map(|a| a.into_common()))
                        .map(|app| app.all_cells().cloned().collect());
                    Ok((state, cells))
                }
            })
            .await?;
        Ok(cells_to_remove)
    }

    /// Add fully constructed cells to the cell map in the Conductor
    pub(super) fn add_cells(&mut self, cells: Vec<Cell>) {
        for cell in cells {
            let cell_id = cell.id().clone();
            tracing::info!(?cell_id, "ADD CELL");
            self.cells.insert(
                cell_id,
                CellItem {
                    cell: Arc::new(cell),
                    _state: CellState { _active: false },
                },
            );
        }
    }

    /// Associate a Cell with an existing App
    pub(super) async fn add_clone_cell_to_app(
        &mut self,
        installed_app_id: &InstalledAppId,
        slot_id: &SlotId,
        properties: YamlProperties,
    ) -> ConductorResult<CellId> {
        let saf_store = &self.saf_store;
        let (_, child_saf) = self
            .update_state_prime(move |mut state| {
                if let Some(app) = state.active_apps.get_mut(installed_app_id) {
                    let slot = app
                        .slots()
                        .get(slot_id)
                        .ok_or_else(|| AppError::SlotIdMissing(slot_id.to_owned()))?;
                    let parent_saf_hash = slot.saf_hash();
                    let saf = saf_store
                        .get(parent_saf_hash)
                        .ok_or_else(|| SafError::SafMissing(parent_saf_hash.to_owned()))?
                        .modify_phenotype(random_uid(), properties)?;
                    Ok((state, saf))
                } else {
                    Err(ConductorError::AppNotActive(installed_app_id.clone()))
                }
            })
            .await?;
        let child_saf_hash = child_saf.saf_hash().to_owned();
        self.register_phenotype(child_saf).await?;
        let (_, cell_id) = self
            .update_state_prime(|mut state| {
                if let Some(app) = state.active_apps.get_mut(installed_app_id) {
                    let agent_key = app.slot(slot_id)?.agent_key().to_owned();
                    let cell_id = CellId::new(child_saf_hash, agent_key);
                    app.add_clone(slot_id, cell_id.clone())?;
                    Ok((state, cell_id))
                } else {
                    Err(ConductorError::AppNotActive(installed_app_id.clone()))
                }
            })
            .await?;
        Ok(cell_id)
    }

    pub(super) async fn load_wasms_into_saf_files(
        &self,
    ) -> ConductorResult<(
        impl IntoIterator<Item = (SafHash, SafFile)>,
        impl IntoIterator<Item = (EntryDefBufferKey, EntryDef)>,
    )> {
        let env = &self.wasm_env;

        // Load out all saf defs
        let (wasm_tasks, defs) = env
            .async_reader(move |txn| {
                let wasm_tasks = aingle_state::saf_def::get_all(&txn)?
                    .into_iter()
                    .map(|saf_def| {
                        // Load all wasms for each saf_def from the wasm db into memory
                        let wasms = saf_def.zomes.clone().into_iter().map(|(zome_name, zome)| {
                            let wasm_hash = zome.wasm_hash(&zome_name)?;
                            aingle_state::wasm::get(&txn, &wasm_hash)?
                                .map(|hashed| hashed.into_content())
                                .ok_or(ConductorError::WasmMissing)
                        });
                        let wasms = wasms.collect::<ConductorResult<Vec<_>>>();
                        async move {
                            let saf_file = SafFile::new(saf_def.into_content(), wasms?).await?;
                            ConductorResult::Ok((saf_file.saf_hash().clone(), saf_file))
                        }
                    })
                    // This needs to happen due to the environment not being Send
                    .collect::<Vec<_>>();
                let defs = aingle_state::entry_def::get_all(&txn)?;
                ConductorResult::Ok((wasm_tasks, defs))
            })
            .await?;
        // try to join all the tasks and return the list of saf files
        let safs = futures::future::try_join_all(wasm_tasks).await?;
        Ok((safs, defs))
    }

    /// Remove cells from the cell map in the Conductor
    pub(super) async fn remove_cells(&mut self, cell_ids: Vec<CellId>) {
        for cell_id in cell_ids {
            if let Some(item) = self.cells.remove(&cell_id) {
                if let Err(err) = item.cell.cleanup().await {
                    tracing::error!("Error cleaning up Cell: {:?}\nCellId: {}", err, cell_id);
                }
            }
        }
    }

    pub(super) async fn add_agent_infos(
        &self,
        agent_infos: Vec<AgentInfoSigned>,
    ) -> ConductorApiResult<()> {
        let mut space_map = HashMap::new();
        for agent_info_signed in agent_infos {
            let space = agent_info_signed.space.clone();
            space_map
                .entry(space)
                .or_insert_with(Vec::new)
                .push(agent_info_signed);
        }
        for (space, agent_infos) in space_map {
            let env = self.p2p_env(space);
            inject_agent_infos(env, agent_infos.iter()).await?;
        }
        Ok(())
    }

    pub(super) fn get_agent_infos(
        &self,
        cell_id: Option<CellId>,
    ) -> ConductorApiResult<Vec<AgentInfoSigned>> {
        match cell_id {
            Some(c) => {
                let (d, a) = c.into_saf_and_agent();
                let space = d.to_kitsune();
                let env = self.p2p_env(space);
                Ok(get_single_agent_info(env.into(), d, a)?
                    .map(|a| vec![a])
                    .unwrap_or_default())
            }
            None => {
                let mut out = Vec::new();
                // collecting so the mutex lock can close
                let envs = self.p2p_env.lock().values().cloned().collect::<Vec<_>>();
                for env in envs {
                    out.append(&mut all_agent_infos(env.into())?);
                }
                Ok(out)
            }
        }
    }

    pub(super) async fn put_wasm(
        &self,
        saf: SafFile,
    ) -> ConductorResult<Vec<(EntryDefBufferKey, EntryDef)>> {
        let env = self.wasm_env.clone();

        let zome_defs = get_entry_defs(saf.clone())?;

        // TODO: PERF: This loop might be slow
        let wasms = futures::future::join_all(
            saf.code()
                .clone()
                .into_values()
                .map(|saf_wasm| SafWasmHashed::from_content(saf_wasm)),
        )
        .await;

        env.async_commit({
            let zome_defs = zome_defs.clone();
            move |txn| {
                for saf_wasm in wasms {
                    if !aingle_state::wasm::contains(txn, saf_wasm.as_hash())? {
                        aingle_state::wasm::put(txn, saf_wasm)?;
                    }
                }

                for (key, entry_def) in zome_defs.clone() {
                    aingle_state::entry_def::put(txn, key, entry_def)?;
                }

                if !aingle_state::saf_def::contains(txn, saf.saf_hash())? {
                    aingle_state::saf_def::put(txn, saf.saf_def().clone())?;
                }
                StateMutationResult::Ok(())
            }
        })
        .await?;

        Ok(zome_defs)
    }

    pub(super) async fn list_cell_ids(&self) -> ConductorResult<Vec<CellId>> {
        Ok(self.cells.keys().cloned().collect())
    }

    pub(super) async fn list_active_apps(&self) -> ConductorResult<Vec<InstalledAppId>> {
        let active_apps = self.get_state().await?.active_apps;
        Ok(active_apps.keys().cloned().collect())
    }

    pub(super) async fn list_apps(
        &self,
        status_filter: Option<AppStatusFilter>,
    ) -> ConductorResult<Vec<InstalledAppInfo>> {
        let conductor_state = self.get_state().await?;

        let apps_ids: Vec<&String> = match status_filter {
            Some(AppStatusFilter::Active) => conductor_state.active_apps.keys().collect(),
            Some(AppStatusFilter::Inactive) => conductor_state.inactive_apps.keys().collect(),
            None => conductor_state
                .active_apps
                .keys()
                .chain(conductor_state.inactive_apps.keys())
                .collect(),
        };

        let apps_info: Vec<InstalledAppInfo> = apps_ids
            .into_iter()
            .filter_map(|app_id| conductor_state.get_app_info(app_id))
            .collect();

        Ok(apps_info)
    }

    pub(super) async fn list_active_apps_for_cell_id(
        &self,
        cell_id: &CellId,
    ) -> ConductorResult<HashSet<InstalledAppId>> {
        let active_apps = self.get_state().await?.active_apps;
        Ok(active_apps
            .iter()
            .filter(|(_, v)| v.all_cells().any(|i| i == cell_id))
            .map(|(k, _)| k)
            .cloned()
            .collect())
    }

    pub(super) async fn dump_cell_state(&self, cell_id: &CellId) -> ConductorApiResult<String> {
        let cell = self.cell_by_id(cell_id)?;
        let arc = cell.env();

        let space = cell_id.saf_hash().to_kitsune();
        let p2p_env = self
            .p2p_env
            .lock()
            .get(&space)
            .cloned()
            .expect("invalid cell space");

        let peer_dump = p2p_agent_store::dump_state(p2p_env.into(), Some(cell_id.clone()))?;
        let source_chain_dump =
            source_chain::dump_state(arc.clone().into(), cell_id.agent_pubkey().clone()).await?;

        let out = JsonDump {
            peer_dump,
            source_chain_dump,
            integration_dump: integration_dump(&arc.clone().into()).await?,
        };
        // Add summary
        let summary = out.to_string();
        let out = (out, summary);
        Ok(serde_json::to_string_pretty(&out)?)
    }

    pub(super) fn p2p_env(&self, space: Arc<KitsuneSpace>) -> EnvWrite {
        let mut p2p_env = self.p2p_env.lock();
        p2p_env
            .entry(space.clone())
            .or_insert_with(move || {
                let root_env_dir = self.root_env_dir.as_ref();
                let keystore = self.keystore.clone();
                EnvWrite::open(root_env_dir, DbKind::P2pAgentStore(space), keystore)
                    .expect("failed to open p2p_agent_store database")
            })
            .clone()
    }

    pub(super) fn p2p_metrics_env(&self, space: Arc<KitsuneSpace>) -> EnvWrite {
        let mut p2p_metrics_env = self.p2p_metrics_env.lock();
        p2p_metrics_env
            .entry(space.clone())
            .or_insert_with(move || {
                let root_env_dir = self.root_env_dir.as_ref();
                let keystore = self.keystore.clone();
                EnvWrite::open(root_env_dir, DbKind::P2pMetrics(space), keystore)
                    .expect("failed to open p2p_metrics database")
            })
            .clone()
    }

    pub(super) fn print_setup(&self) {
        use std::fmt::Write;
        let mut out = String::new();
        for port in &self.admin_websocket_ports {
            writeln!(&mut out, "###ADMIN_PORT:{}###", port).expect("Can't write setup to std out");
        }
        println!("\n###AINGLE_SETUP###\n{}###AINGLE_SETUP_END###", out);
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub(super) async fn get_state_from_handle(&self) -> ConductorResult<ConductorState> {
        self.get_state().await
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub(super) async fn add_test_app_interface<I: Into<AppInterfaceId>>(
        &mut self,
        id: I,
    ) -> ConductorResult<()> {
        let id = id.into();
        let (signal_tx, _r) = tokio::sync::broadcast::channel(1000);
        if self.app_interfaces.contains_key(&id) {
            return Err(ConductorError::AppInterfaceIdCollision(id));
        }
        let _ = self
            .app_interfaces
            .insert(id, AppInterfaceRuntime::Test { signal_tx });
        Ok(())
    }
}

/// Dump the integration json state.
pub async fn integration_dump(vault: &EnvRead) -> ConductorApiResult<IntegrationStateDump> {
    vault
        .async_reader(move |txn| {
            let integrated = txn.query_row(
                "SELECT count(hash) FROM SgdOp WHERE when_integrated IS NOT NULL",
                [],
                |row| row.get(0),
            )?;
            let integration_limbo = txn.query_row(
            "SELECT count(hash) FROM SgdOp WHERE when_integrated IS NULL AND validation_stage = 3",
            [],
            |row| row.get(0),
        )?;
            let validation_limbo = txn.query_row(
                "
                SELECT count(hash) FROM SgdOp
                WHERE when_integrated IS NULL
                AND (
                    (is_authored = 1 AND validation_stage IS NOT NULL AND validation_stage < 3)
                    OR
                    (is_authored = 0 AND (validation_stage IS NULL OR validation_stage < 3))
                )
                ",
                [],
                |row| row.get(0),
            )?;
            ConductorApiResult::Ok(IntegrationStateDump {
                validation_limbo,
                integration_limbo,
                integrated,
            })
        })
        .await
}

//-----------------------------------------------------------------------------
// Private methods
//-----------------------------------------------------------------------------

impl<DS> Conductor<DS>
where
    DS: SafStore + 'static,
{
    #[allow(clippy::too_many_arguments)]
    async fn new(
        env: EnvWrite,
        wasm_env: EnvWrite,
        p2p_env: Arc<parking_lot::Mutex<HashMap<Arc<KitsuneSpace>, EnvWrite>>>,
        p2p_metrics_env: Arc<parking_lot::Mutex<HashMap<Arc<KitsuneSpace>, EnvWrite>>>,
        saf_store: DS,
        keystore: KeystoreSender,
        root_env_dir: EnvironmentRootPath,
        aingle_p2p: aingle_p2p::AIngleP2pRef,
        #[cfg(feature = "ai-integration")] ai_service: Option<Arc<parking_lot::RwLock<AiService>>>,
    ) -> ConductorResult<Self> {
        Ok(Self {
            env,
            wasm_env,
            p2p_env,
            p2p_metrics_env,
            caches: parking_lot::Mutex::new(HashMap::new()),
            cells: HashMap::new(),
            shutting_down: false,
            app_interfaces: HashMap::new(),
            task_manager: None,
            admin_websocket_ports: Vec::new(),
            saf_store,
            keystore,
            root_env_dir,
            aingle_p2p,
            #[cfg(feature = "ai-integration")]
            ai_service,
        })
    }

    pub(super) async fn start_task_manager(
        &mut self,
        handle: ConductorHandle,
    ) -> ConductorResult<()> {
        if self.task_manager.is_some() {
            panic!("Cannot start task manager twice");
        }
        let (task_add_sender, run_handle) = spawn_task_manager(handle);
        let (task_stop_broadcaster, _) = tokio::sync::broadcast::channel::<()>(1);
        self.task_manager = Some(TaskManagerClient::new(
            task_add_sender,
            task_stop_broadcaster,
            run_handle,
        ));
        Ok(())
    }

    pub(super) async fn get_state(&self) -> ConductorResult<ConductorState> {
        self.env.conn()?.with_reader(|txn| {
            let state = txn
                .query_row("SELECT blob FROM ConductorState WHERE id = 1", [], |row| {
                    row.get("blob")
                })
                .optional()?;
            let state = match state {
                Some(state) => from_blob(state)?,
                None => ConductorState::default(),
            };
            Ok(state)
        })
    }

    /// Update the internal state with a pure function mapping old state to new
    async fn update_state<F: Send>(&self, f: F) -> ConductorResult<ConductorState>
    where
        F: FnOnce(ConductorState) -> ConductorResult<ConductorState> + 'static,
    {
        let (state, _) = self.update_state_prime(|s| Ok((f(s)?, ()))).await?;
        Ok(state)
    }

    /// Update the internal state with a pure function mapping old state to new,
    /// which may also produce an output value which will be the output of
    /// this function
    async fn update_state_prime<F, O>(&self, f: F) -> ConductorResult<(ConductorState, O)>
    where
        F: FnOnce(ConductorState) -> ConductorResult<(ConductorState, O)>,
        O: Send,
    {
        self.check_running()?;
        let output = self
            .env
            .async_commit_in_place(move |txn| {
                let state = txn
                    .query_row("SELECT blob FROM ConductorState WHERE id = 1", [], |row| {
                        row.get("blob")
                    })
                    .optional()?;
                let state = match state {
                    Some(state) => from_blob(state)?,
                    None => ConductorState::default(),
                };
                let (new_state, output) = f(state)?;
                mutations::insert_conductor_state(txn, (&new_state).try_into()?)?;
                Result::<_, ConductorError>::Ok((new_state, output))
            })
            .await?;
        Ok(output)
    }

    fn add_admin_port(&mut self, port: u16) {
        self.admin_websocket_ports.push(port);
    }

    /// Sends a JoinHandle to the TaskManager task to be managed
    async fn manage_task(&mut self, handle: ManagedTaskAdd) -> ConductorResult<()> {
        self.task_manager
            .as_ref()
            .expect("Task manager not initialized")
            .task_add_sender()
            .send(handle)
            .await
            .map_err(|e| ConductorError::SubmitTaskError(format!("{}", e)))
    }
}

mod builder {
    use super::*;
    use crate::conductor::saf_store::RealSafStore;
    use crate::conductor::ConductorHandle;
    #[cfg(feature = "ai-integration")]
    use aingle_ai::AiConfig;
    use aingle_sqlite::db::DbKind;
    #[cfg(any(test, feature = "test_utils"))]
    use aingle_state::test_utils::TestEnvs;

    /// A configurable Builder for Conductor and sometimes ConductorHandle
    #[derive(Default)]
    pub struct ConductorBuilder<DS = RealSafStore> {
        /// The configuration
        pub config: ConductorConfig,
        /// The SafStore (mockable)
        pub saf_store: DS,
        /// Optional keystore override
        pub keystore: Option<KeystoreSender>,
        #[cfg(any(test, feature = "test_utils"))]
        /// Optional state override (for testing)
        pub state: Option<ConductorState>,
        #[cfg(any(test, feature = "test_utils"))]
        /// Optional handle mock (for testing)
        pub mock_handle: Option<MockConductorHandleT>,
        #[cfg(feature = "ai-integration")]
        /// Optional AI configuration for intelligent validation
        pub ai_config: Option<AiConfig>,
    }

    impl ConductorBuilder {
        /// Default ConductorBuilder
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl ConductorBuilder<MockSafStore> {
        /// ConductorBuilder using mocked SafStore, for testing
        pub fn with_mock_saf_store(saf_store: MockSafStore) -> ConductorBuilder<MockSafStore> {
            Self {
                saf_store,
                ..Default::default()
            }
        }
    }

    impl<DS> ConductorBuilder<DS>
    where
        DS: SafStore + 'static,
    {
        /// Set the ConductorConfig used to build this Conductor
        pub fn config(mut self, config: ConductorConfig) -> Self {
            self.config = config;
            self
        }

        /// Enable AI-powered validation and consensus
        #[cfg(feature = "ai-integration")]
        pub fn with_ai(mut self, config: AiConfig) -> Self {
            self.ai_config = Some(config);
            self
        }

        /// Enable AI with default configuration
        #[cfg(feature = "ai-integration")]
        pub fn with_ai_default(mut self) -> Self {
            self.ai_config = Some(AiConfig::default());
            self
        }

        /// Enable AI with IoT-optimized configuration
        #[cfg(feature = "ai-integration")]
        pub fn with_ai_iot(mut self) -> Self {
            self.ai_config = Some(AiConfig::iot());
            self
        }

        /// Initialize a "production" Conductor
        pub async fn build(self) -> ConductorResult<ConductorHandle> {
            cfg_if::cfg_if! {
                // if mock_handle is specified, return that instead of
                // a real handle
                if #[cfg(test)] {
                    if let Some(handle) = self.mock_handle {
                        return Ok(Arc::new(handle));
                    }
                }
            }

            tracing::info!(?self.config);

            let keystore = if let Some(keystore) = self.keystore {
                keystore
            } else if self.config.use_dangerous_test_keystore {
                let keystore = spawn_test_keystore().await?;
                // pre-populate with our two fixture agent keypairs
                keystore
                    .generate_sign_keypair_from_pure_entropy()
                    .await
                    .unwrap();
                keystore
                    .generate_sign_keypair_from_pure_entropy()
                    .await
                    .unwrap();
                keystore
            } else {
                spawn_lair_keystore(self.config.keystore_path.as_deref()).await?
            };
            let env_path = self.config.environment_path.clone();

            let environment =
                EnvWrite::open(env_path.as_ref(), DbKind::Conductor, keystore.clone())?;

            let wasm_environment =
                EnvWrite::open(env_path.as_ref(), DbKind::Wasm, keystore.clone())?;

            let p2p_env = Arc::new(parking_lot::Mutex::new(HashMap::new()));
            let p2p_metrics_env = Arc::new(parking_lot::Mutex::new(HashMap::new()));

            #[cfg(any(test, feature = "test_utils"))]
            let state = self.state;

            #[cfg(feature = "ai-integration")]
            let ai_config = self.ai_config;

            let Self {
                saf_store, config, ..
            } = self;

            let network_config = match &config.network {
                None => aingle_p2p::kitsune_p2p::KitsuneP2pConfig::default(),
                Some(config) => config.clone(),
            };
            let (cert_digest, cert, cert_priv_key) =
                keystore.get_or_create_first_tls_cert().await?;
            let tls_config = aingle_p2p::kitsune_p2p::dependencies::kitsune_p2p_proxy::TlsConfig {
                cert,
                cert_priv_key,
                cert_digest,
            };
            let (aingle_p2p, p2p_evt) =
                aingle_p2p::spawn_aingle_p2p(network_config, tls_config).await?;

            // Create AI service if configured
            #[cfg(feature = "ai-integration")]
            let ai_service = ai_config.map(|config| {
                tracing::info!("Initializing AI service with config: {:?}", config);
                Arc::new(parking_lot::RwLock::new(AiService::new(config)))
            });

            let conductor = Conductor::new(
                environment,
                wasm_environment,
                p2p_env,
                p2p_metrics_env,
                saf_store,
                keystore,
                env_path,
                aingle_p2p,
                #[cfg(feature = "ai-integration")]
                ai_service,
            )
            .await?;

            #[cfg(any(test, feature = "test_utils"))]
            let conductor = Self::update_fake_state(state, conductor).await?;

            Self::finish(conductor, config, p2p_evt).await
        }

        async fn finish(
            conductor: Conductor<DS>,
            conductor_config: ConductorConfig,
            p2p_evt: aingle_p2p::event::AIngleP2pEventReceiver,
        ) -> ConductorResult<ConductorHandle> {
            // Get data before handle
            let keystore = conductor.keystore.clone();
            let aingle_p2p = conductor.aingle_p2p.clone();

            // Create handle
            let handle: ConductorHandle = Arc::new(ConductorHandleImpl {
                conductor: RwLock::new(conductor),
                keystore,
                aingle_p2p,
            });

            let configs = conductor_config.admin_interfaces.unwrap_or_default();
            handle.clone().initialize_conductor(configs).await?;

            handle.load_safs().await?;

            tokio::task::spawn(p2p_event_task(p2p_evt, handle.clone()));

            let cell_startup_errors = handle.clone().setup_cells().await?;

            // TODO: This should probably be emitted over the admin interface
            if !cell_startup_errors.is_empty() {
                error!(
                    msg = "Failed to create the following active apps",
                    ?cell_startup_errors
                );
            }

            handle.print_setup().await;

            Ok(handle)
        }

        /// Pass a test keystore in, to ensure that generated test agents
        /// are actually available for signing (especially for tryorama compat)
        pub fn with_keystore(mut self, keystore: KeystoreSender) -> Self {
            self.keystore = Some(keystore);
            self
        }

        #[cfg(any(test, feature = "test_utils"))]
        /// Sets some fake conductor state for tests
        pub fn fake_state(mut self, state: ConductorState) -> Self {
            self.state = Some(state);
            self
        }

        /// Pass a mock handle in, which will be returned regardless of whatever
        /// else happens to this builder
        #[cfg(any(test, feature = "test_utils"))]
        pub fn with_mock_handle(mut self, handle: MockConductorHandleT) -> Self {
            self.mock_handle = Some(handle);
            self
        }

        #[cfg(any(test, feature = "test_utils"))]
        async fn update_fake_state(
            state: Option<ConductorState>,
            conductor: Conductor<DS>,
        ) -> ConductorResult<Conductor<DS>> {
            if let Some(state) = state {
                conductor.update_state(move |_| Ok(state)).await?;
            }
            Ok(conductor)
        }

        /// Build a Conductor with a test environment
        #[cfg(any(test, feature = "test_utils"))]
        pub async fn test(self, envs: &TestEnvs) -> ConductorResult<ConductorHandle> {
            let keystore = envs.conductor().keystore().clone();

            let (aingle_p2p, p2p_evt) = aingle_p2p::spawn_aingle_p2p(
                self.config.network.clone().unwrap_or_default(),
                aingle_p2p::kitsune_p2p::dependencies::kitsune_p2p_proxy::TlsConfig::new_ephemeral(
                )
                .await
                .unwrap(),
            )
            .await?;

            // Create AI service if configured (in test mode)
            #[cfg(feature = "ai-integration")]
            let ai_service = self
                .ai_config
                .map(|config| Arc::new(parking_lot::RwLock::new(AiService::new(config))));

            let conductor = Conductor::new(
                envs.conductor(),
                envs.wasm(),
                envs.p2p(),
                envs.p2p_metrics(),
                self.saf_store,
                keystore,
                envs.tempdir().path().to_path_buf().into(),
                aingle_p2p,
                #[cfg(feature = "ai-integration")]
                ai_service,
            )
            .await?;

            let conductor = Self::update_fake_state(self.state, conductor).await?;

            Self::finish(conductor, self.config, p2p_evt).await
        }
    }
}

#[instrument(skip(p2p_evt, handle))]
async fn p2p_event_task(
    p2p_evt: aingle_p2p::event::AIngleP2pEventReceiver,
    handle: ConductorHandle,
) {
    /// The number of events we allow to run in parallel before
    /// starting to await on the join handles.
    const NUM_PARALLEL_EVTS: usize = 100;
    p2p_evt
        .for_each_concurrent(NUM_PARALLEL_EVTS, |evt| {
            let handle = handle.clone();
            async move {
                let cell_id =
                    CellId::new(evt.saf_hash().clone(), evt.target_agent_as_ref().clone());
                if let Err(e) = handle.dispatch_aingle_p2p_event(&cell_id, evt).await {
                    tracing::error!(
                        message = "error dispatching network event",
                        error = ?e,
                    );
                }
            }
            .in_current_span()
        })
        .await;

    tracing::warn!("p2p_event_task has ended");
}

#[cfg(test)]
#[allow(missing_docs)]
pub mod tests;
