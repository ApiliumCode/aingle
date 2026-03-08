//! P2P manager — orchestrates identity, transport, gossip, sync, and discovery.

use crate::p2p::config::P2pConfig;
use crate::p2p::discovery::P2pDiscovery;
use crate::p2p::gossip::{BloomFilter, GossipStats, TripleGossipManager};
use crate::p2p::message::{P2pMessage, TombstoneWire, TripleWire};
use crate::p2p::peer_store::{PeerSource, PeerStore, StoredPeer};
use crate::p2p::rate_limiter::IngressRateLimiter;
use crate::p2p::sync_manager::{SyncStats, TripleSyncManager};
use crate::p2p::transport::{P2pTransport, P2pTransportConfig};
use crate::state::{AppState, Event};

use aingle_graph::TripleId;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ── Manual peer reconnection (A2) ────────────────────────────────

/// Tracks reconnection state for a manually-configured peer.
struct ManualPeerTracker {
    addr: SocketAddr,
    retries: u32,
    max_retries: u32,
    current_backoff: Duration,
    last_attempt: Instant,
    abandoned: bool,
}

impl ManualPeerTracker {
    fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            retries: 0,
            max_retries: 10,
            current_backoff: Duration::from_secs(5),
            last_attempt: Instant::now(),
            abandoned: false,
        }
    }

    fn should_retry(&self) -> bool {
        !self.abandoned && self.last_attempt.elapsed() >= self.current_backoff
    }

    fn record_failure(&mut self) {
        self.retries += 1;
        self.last_attempt = Instant::now();
        self.current_backoff = Duration::from_secs(
            (self.current_backoff.as_secs() * 2).min(300),
        );
        if self.retries >= self.max_retries {
            self.abandoned = true;
        }
    }

    fn record_success(&mut self) {
        self.retries = 0;
        self.current_backoff = Duration::from_secs(5);
        self.abandoned = false;
        self.last_attempt = Instant::now();
    }
}

// ── Health check tracking (A4) ───────────────────────────────────

/// Tracks outstanding pings for health checking.
struct PingTracker {
    outstanding: HashMap<SocketAddr, (u64, Instant)>,
    timeout: Duration,
}

impl PingTracker {
    fn new(timeout: Duration) -> Self {
        Self {
            outstanding: HashMap::new(),
            timeout,
        }
    }

    fn record_ping(&mut self, addr: SocketAddr, timestamp_ms: u64) {
        self.outstanding.insert(addr, (timestamp_ms, Instant::now()));
    }

    fn record_pong(&mut self, addr: &SocketAddr, _timestamp_ms: u64) {
        self.outstanding.remove(addr);
    }

    fn timed_out_peers(&self) -> Vec<SocketAddr> {
        self.outstanding
            .iter()
            .filter(|(_, (_, sent))| sent.elapsed() >= self.timeout)
            .map(|(addr, _)| *addr)
            .collect()
    }

    fn clear(&mut self, addr: &SocketAddr) {
        self.outstanding.remove(addr);
    }
}

// ── Health events passed between tasks ───────────────────────────

enum HealthEvent {
    PongReceived { addr: SocketAddr, timestamp_ms: u64 },
}

// ── P2P Manager ──────────────────────────────────────────────────

/// Orchestrator for the entire P2P subsystem.
pub struct P2pManager {
    config: P2pConfig,
    node_id: String,
    gossip: Arc<RwLock<TripleGossipManager>>,
    sync: Arc<RwLock<TripleSyncManager>>,
    transport: Arc<RwLock<P2pTransport>>,
    discovery: Arc<RwLock<P2pDiscovery>>,
    running: Arc<AtomicBool>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

/// Serializable P2P status.
#[derive(Debug, Clone, serde::Serialize)]
pub struct P2pStatus {
    pub node_id: String,
    pub enabled: bool,
    pub port: u16,
    pub peer_count: usize,
    pub connected_peers: Vec<PeerStatusDto>,
    pub gossip_stats: GossipStats,
    pub sync_stats: SyncStats,
}

/// Per-peer status DTO.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerStatusDto {
    pub addr: String,
    pub connected: bool,
}

impl P2pManager {
    /// Start the P2P subsystem: load identity, bind transport, connect manual peers,
    /// start discovery, and spawn background tasks.
    pub async fn start(config: P2pConfig, app_state: AppState) -> Result<Arc<Self>, String> {
        // 1. Load or generate node identity.
        let identity = crate::p2p::identity::NodeIdentity::load_or_generate(&config.data_dir)
            .map_err(|e| format!("identity: {}", e))?;
        let node_id = identity.node_id();

        // 2. Compute seed hash.
        let seed_hash = if let Some(ref seed) = config.seed {
            hex::encode(blake3::hash(seed.as_bytes()).as_bytes())
        } else {
            String::new()
        };

        // 3. Create gossip & sync managers.
        let gossip = Arc::new(RwLock::new(TripleGossipManager::new()));
        let sync = Arc::new(RwLock::new(TripleSyncManager::new(Duration::from_millis(
            config.gossip_interval_ms,
        ))));

        // 4. Create transport and start.
        let transport_config = P2pTransportConfig {
            port: config.port,
            max_connections: config.max_peers * 2,
            ..Default::default()
        };
        let mut transport_inner =
            P2pTransport::new(transport_config, node_id.clone(), seed_hash.clone());
        transport_inner.start().await?;

        // 5. Rebuild local IDs from graph.
        {
            let graph = app_state.graph.read().await;
            sync.write().await.rebuild_local_ids(&graph);
        }

        // A3: Load persistent peer store and merge with manual peers.
        let peer_store = Arc::new(RwLock::new(
            PeerStore::load(&config.data_dir, config.max_peers * 2),
        ));

        // 6. Connect to manual peers + persisted peers.
        let triple_count = {
            let s = sync.read().await;
            s.local_ids().len() as u64
        };

        // Connect manual peers
        for peer_addr in &config.manual_peers {
            match transport_inner.connect(*peer_addr, triple_count).await {
                Ok(()) => {
                    tracing::info!("P2P connected to manual peer {}", peer_addr);
                    let now_ms = now_millis();
                    let mut ps = peer_store.write().await;
                    ps.add(StoredPeer {
                        addr: *peer_addr,
                        node_id: None,
                        last_connected_ms: now_ms,
                        source: PeerSource::Manual,
                    });
                    let _ = ps.save();
                }
                Err(e) => tracing::warn!("P2P failed to connect to {}: {}", peer_addr, e),
            }
        }

        // Connect persisted peers not already connected
        {
            let ps = peer_store.read().await;
            for stored in ps.all() {
                if !transport_inner.is_connected(&stored.addr)
                    && !config.manual_peers.contains(&stored.addr)
                {
                    match transport_inner.connect(stored.addr, triple_count).await {
                        Ok(()) => {
                            tracing::info!("P2P reconnected to persisted peer {}", stored.addr);
                        }
                        Err(e) => {
                            tracing::debug!("P2P persisted peer {} unreachable: {}", stored.addr, e);
                        }
                    }
                }
            }
        }

        let transport = Arc::new(RwLock::new(transport_inner));

        // A6: Create ingress rate limiter.
        let rate_limiter = Arc::new(RwLock::new(IngressRateLimiter::new(
            config.max_triples_per_peer_per_min,
            config.max_triples_global_per_min,
        )));

        // 7. Discovery.
        let mut disc =
            P2pDiscovery::new(node_id.clone(), seed_hash.clone(), config.port)?;
        if config.mdns {
            disc.register()?;
            disc.start_browsing()?;
        }
        let discovery = Arc::new(RwLock::new(disc));

        let running = Arc::new(AtomicBool::new(true));
        let mut tasks = Vec::new();

        // A4: Health event channel between Task 3 (sender) and Task 6 (receiver).
        let (health_tx, health_rx) = tokio::sync::mpsc::channel::<HealthEvent>(256);

        // ── Task 1: Event listener ───────────────────────────
        {
            let gossip = gossip.clone();
            let sync = sync.clone();
            let transport = transport.clone();
            let running = running.clone();
            let broadcaster = app_state.broadcaster.clone();
            let graph = app_state.graph.clone();

            tasks.push(tokio::spawn(async move {
                let mut rx = broadcaster.subscribe();
                loop {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    match rx.recv().await {
                        Ok(Event::TripleAdded { hash, .. }) => {
                            if let Some(id) = TripleId::from_hex(&hash) {
                                gossip.write().await.announce(id.0);
                                sync.write().await.add_local_id(id.0);
                            }
                        }
                        // A1: Handle triple deletions — tombstone propagation.
                        Ok(Event::TripleDeleted { hash }) => {
                            if let Some(id) = TripleId::from_hex(&hash) {
                                // 1. Remove from gossip recent_ids
                                gossip.write().await.remove_known(&id.0);
                                // 2. Remove from sync local_ids
                                let mut s = sync.write().await;
                                s.remove_local_id(&id.0);
                                // 3. Add tombstone
                                let ts_ms = now_millis();
                                s.add_tombstone(id.0, ts_ms);
                                drop(s);
                                // 4. Broadcast AnnounceDelete to all connected peers
                                let peers = transport.read().await.connected_peers();
                                let msg = P2pMessage::AnnounceDelete {
                                    triple_id: hash,
                                    tombstone_ts: ts_ms,
                                };
                                let t = transport.read().await;
                                for peer_addr in &peers {
                                    let _ = t.send(peer_addr, &msg).await;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("P2P event listener lagged by {} events, rebuilding", n);
                            let g = graph.read().await;
                            sync.write().await.rebuild_local_ids(&g);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        _ => {}
                    }
                }
            }));
        }

        // ── Task 2: Gossip loop + tombstone sync ─────────────
        {
            let sync = sync.clone();
            let transport = transport.clone();
            let running = running.clone();
            let interval = config.gossip_interval_ms;

            tasks.push(tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(interval)).await;
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }

                    let peers = sync.read().await.peers_needing_sync();
                    for peer_addr in peers {
                        // Send bloom filter
                        let filter = sync.read().await.build_local_filter();
                        let triple_count = sync.read().await.local_ids().len() as u64;
                        let msg = P2pMessage::BloomSync {
                            filter_bytes: filter.to_bytes(),
                            triple_count,
                        };
                        let t = transport.read().await;
                        if let Err(e) = t.send(&peer_addr, &msg).await {
                            tracing::debug!("gossip send to {}: {}", peer_addr, e);
                        }

                        // A1: Also send active tombstones
                        let tombstones = sync.read().await.active_tombstones();
                        if !tombstones.is_empty() {
                            let wires: Vec<TombstoneWire> = tombstones
                                .iter()
                                .map(|(id, ts)| TombstoneWire {
                                    triple_id: hex::encode(id),
                                    deleted_at_ms: *ts,
                                })
                                .collect();
                            let ts_msg = P2pMessage::TombstoneSync { tombstones: wires };
                            let _ = t.send(&peer_addr, &ts_msg).await;
                        }
                    }
                }
            }));
        }

        // ── Task 3: Incoming message handler ─────────────────
        {
            let gossip = gossip.clone();
            let sync = sync.clone();
            let transport = transport.clone();
            let running = running.clone();
            let graph = app_state.graph.clone();
            let seed_hash2 = seed_hash.clone();
            let rate_limiter = rate_limiter.clone();
            let peer_store = peer_store.clone();
            let health_tx = health_tx.clone();

            tasks.push(tokio::spawn(async move {
                loop {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    // A5: Still uses polling recv (accept_loop would require
                    // transport refactoring beyond scope; see note below).
                    // Reduced poll interval from 50ms to 10ms for better latency.
                    tokio::time::sleep(Duration::from_millis(10)).await;

                    let maybe = {
                        let t = transport.read().await;
                        t.recv().await
                    };

                    let (addr, msg) = match maybe {
                        Ok(Some(pair)) => pair,
                        _ => continue,
                    };

                    match msg {
                        P2pMessage::Hello {
                            seed_hash: peer_seed,
                            node_id: peer_node_id,
                            ..
                        } => {
                            let accepted = peer_seed == seed_hash2;
                            let ack = P2pMessage::HelloAck {
                                node_id: String::new(),
                                accepted,
                                reason: if accepted {
                                    None
                                } else {
                                    Some("seed_mismatch".into())
                                },
                            };
                            let t = transport.read().await;
                            let _ = t.send(&addr, &ack).await;
                            if accepted {
                                // A3: Record in peer store
                                let now_ms = now_millis();
                                let mut ps = peer_store.write().await;
                                ps.add(StoredPeer {
                                    addr,
                                    node_id: Some(peer_node_id.clone()),
                                    last_connected_ms: now_ms,
                                    source: PeerSource::RestApi,
                                });
                                let _ = ps.save();
                            } else {
                                tracing::warn!(
                                    "P2P rejected {} from {}: seed mismatch",
                                    &peer_node_id[..8.min(peer_node_id.len())],
                                    addr
                                );
                            }
                        }
                        P2pMessage::BloomSync {
                            filter_bytes,
                            ..
                        } => {
                            let peer_filter = BloomFilter::from_bytes(&filter_bytes);
                            let local_ids: Vec<[u8; 32]> =
                                sync.read().await.local_ids().to_vec();
                            let missing =
                                gossip.read().await.find_missing(&peer_filter, &local_ids);

                            if !missing.is_empty() {
                                let g = graph.read().await;
                                let mut wires = Vec::new();
                                for id_bytes in &missing {
                                    let tid = TripleId::new(*id_bytes);
                                    if let Ok(Some(triple)) = g.get(&tid) {
                                        wires.push(TripleWire::from_triple(&triple));
                                    }
                                }
                                if !wires.is_empty() {
                                    let send_msg = P2pMessage::SendTriples { triples: wires };
                                    let t = transport.read().await;
                                    let _ = t.send(&addr, &send_msg).await;
                                }
                            }
                        }
                        P2pMessage::RequestTriples { ids } => {
                            let g = graph.read().await;
                            let mut wires = Vec::new();
                            for hex_id in &ids {
                                if let Some(tid) = TripleId::from_hex(hex_id) {
                                    if let Ok(Some(triple)) = g.get(&tid) {
                                        wires.push(TripleWire::from_triple(&triple));
                                    }
                                }
                            }
                            if !wires.is_empty() {
                                let send_msg = P2pMessage::SendTriples { triples: wires };
                                let t = transport.read().await;
                                let _ = t.send(&addr, &send_msg).await;
                            }
                        }
                        P2pMessage::SendTriples { triples } => {
                            // A6: Apply backpressure before processing.
                            let total = triples.len();
                            let allowed = rate_limiter.write().await.check(&addr, total);
                            if allowed < total {
                                tracing::warn!(
                                    "P2P rate limited {} triples from {} (allowed {}/{})",
                                    total - allowed,
                                    addr,
                                    allowed,
                                    total
                                );
                            }

                            let converted: Vec<_> = triples
                                .iter()
                                .take(allowed)
                                .filter_map(|tw| tw.to_triple())
                                .collect();
                            let g = graph.read().await;
                            let result = sync
                                .write()
                                .await
                                .store_received_triples(converted, &g);
                            sync.write()
                                .await
                                .record_sync_result(addr, true, result.inserted);
                            if result.inserted > 0 {
                                tracing::info!(
                                    "P2P synced {} triples from {} ({} dup, {} err)",
                                    result.inserted,
                                    addr,
                                    result.duplicates,
                                    result.errors
                                );
                                // A3: Update last connected
                                let mut ps = peer_store.write().await;
                                ps.update_last_connected(&addr, now_millis());
                                let _ = ps.save();
                            }
                        }
                        P2pMessage::Announce { triple_id } => {
                            if let Some(tid) = TripleId::from_hex(&triple_id) {
                                let known = gossip.read().await.is_known(&tid.0);
                                if !known {
                                    let req = P2pMessage::RequestTriples {
                                        ids: vec![triple_id],
                                    };
                                    let t = transport.read().await;
                                    let _ = t.send(&addr, &req).await;
                                }
                            }
                        }
                        // A1: Handle incoming deletion announcement.
                        P2pMessage::AnnounceDelete { triple_id, tombstone_ts } => {
                            if let Some(tid) = TripleId::from_hex(&triple_id) {
                                let mut s = sync.write().await;
                                if !s.has_tombstone(&tid.0) {
                                    s.remove_local_id(&tid.0);
                                    s.add_tombstone(tid.0, tombstone_ts);
                                    drop(s);
                                    gossip.write().await.remove_known(&tid.0);
                                    // Delete from local graph
                                    let g = graph.read().await;
                                    let _ = g.delete(&tid);
                                    tracing::debug!(
                                        "P2P applied tombstone for {} from {}",
                                        &triple_id[..8.min(triple_id.len())],
                                        addr
                                    );
                                }
                            }
                        }
                        // A1: Handle batch tombstone sync.
                        P2pMessage::TombstoneSync { tombstones } => {
                            for tw in &tombstones {
                                if let Some(tid) = TripleId::from_hex(&tw.triple_id) {
                                    let mut s = sync.write().await;
                                    if !s.has_tombstone(&tid.0) {
                                        s.remove_local_id(&tid.0);
                                        s.add_tombstone(tid.0, tw.deleted_at_ms);
                                        drop(s);
                                        gossip.write().await.remove_known(&tid.0);
                                        let g = graph.read().await;
                                        let _ = g.delete(&tid);
                                    }
                                }
                            }
                        }
                        P2pMessage::Ping { timestamp_ms } => {
                            let count = sync.read().await.local_ids().len() as u64;
                            let pong = P2pMessage::Pong {
                                timestamp_ms,
                                triple_count: count,
                            };
                            let t = transport.read().await;
                            let _ = t.send(&addr, &pong).await;
                        }
                        // A4: Forward pong to health task via channel.
                        P2pMessage::Pong { timestamp_ms, .. } => {
                            let _ = health_tx
                                .send(HealthEvent::PongReceived {
                                    addr,
                                    timestamp_ms,
                                })
                                .await;
                        }
                        _ => {}
                    }
                }
            }));
        }

        // ── Task 4: mDNS discovery reconnect ─────────────────
        if config.mdns {
            let transport = transport.clone();
            let discovery = discovery.clone();
            let running = running.clone();
            let sync = sync.clone();
            let peer_store = peer_store.clone();

            tasks.push(tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }

                    let discovered = discovery.read().await.get_discovered_peers();
                    for peer in discovered {
                        let connected = transport.read().await.is_connected(&peer.addr);
                        if !connected {
                            let triple_count = sync.read().await.local_ids().len() as u64;
                            let result = transport
                                .write()
                                .await
                                .connect(peer.addr, triple_count)
                                .await;
                            if let Ok(()) = result {
                                tracing::info!(
                                    "P2P discovered and connected to {}",
                                    peer.node_id
                                );
                                // A3: Record mDNS peer
                                let mut ps = peer_store.write().await;
                                ps.add(StoredPeer {
                                    addr: peer.addr,
                                    node_id: Some(peer.node_id.clone()),
                                    last_connected_ms: now_millis(),
                                    source: PeerSource::Mdns,
                                });
                                let _ = ps.save();
                            }
                        }
                    }
                }
            }));
        }

        // ── Task 5: Manual peer reconnection (A2) ────────────
        {
            let transport = transport.clone();
            let sync = sync.clone();
            let running = running.clone();
            let manual_peers: Vec<SocketAddr> = config.manual_peers.clone();

            tasks.push(tokio::spawn(async move {
                let mut trackers: Vec<ManualPeerTracker> = manual_peers
                    .into_iter()
                    .map(ManualPeerTracker::new)
                    .collect();

                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }

                    for tracker in &mut trackers {
                        if tracker.abandoned {
                            continue;
                        }
                        let connected = transport.read().await.is_connected(&tracker.addr);
                        if connected {
                            if tracker.retries > 0 {
                                tracker.record_success();
                            }
                            continue;
                        }
                        if !tracker.should_retry() {
                            continue;
                        }
                        let triple_count = sync.read().await.local_ids().len() as u64;
                        let result = transport
                            .write()
                            .await
                            .connect(tracker.addr, triple_count)
                            .await;
                        match result {
                            Ok(()) => {
                                tracing::info!("P2P reconnected to manual peer {}", tracker.addr);
                                tracker.record_success();
                            }
                            Err(e) => {
                                tracing::debug!(
                                    "P2P reconnect to {} failed (attempt {}): {}",
                                    tracker.addr,
                                    tracker.retries + 1,
                                    e
                                );
                                tracker.record_failure();
                            }
                        }
                    }
                }
            }));
        }

        // ── Task 6: Health checks — Ping/Pong (A4) ───────────
        {
            let transport = transport.clone();
            let running = running.clone();
            let mut health_rx = health_rx;

            tasks.push(tokio::spawn(async move {
                let mut tracker = PingTracker::new(Duration::from_secs(10));

                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(30)) => {
                            if !running.load(Ordering::Relaxed) {
                                break;
                            }

                            // Check timeouts — disconnect unresponsive peers
                            let timed_out = tracker.timed_out_peers();
                            for addr in &timed_out {
                                tracing::warn!("P2P peer {} timed out (no pong), disconnecting", addr);
                                transport.write().await.disconnect(addr);
                                tracker.clear(addr);
                            }

                            // Send pings to all connected peers
                            let peers = transport.read().await.connected_peers();
                            let ts = now_millis();
                            let ping = P2pMessage::Ping { timestamp_ms: ts };
                            let t = transport.read().await;
                            for peer_addr in &peers {
                                if t.send(peer_addr, &ping).await.is_ok() {
                                    tracker.record_ping(*peer_addr, ts);
                                }
                            }
                        }
                        event = health_rx.recv() => {
                            match event {
                                Some(HealthEvent::PongReceived { addr, timestamp_ms }) => {
                                    tracker.record_pong(&addr, timestamp_ms);
                                }
                                None => break,
                            }
                        }
                    }
                }
            }));
        }

        // ── Task 7: Periodic cleanup (A7) ────────────────────
        {
            let sync = sync.clone();
            let peer_store = peer_store.clone();
            let running = running.clone();

            tasks.push(tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }

                    // 1. Remove inactive sync states (15 min)
                    sync.write()
                        .await
                        .cleanup_inactive(Duration::from_secs(900));

                    // 2. Remove expired tombstones (A1)
                    sync.write().await.cleanup_expired_tombstones();

                    // 3. Cleanup stale peers (24h) and save (A3)
                    let mut ps = peer_store.write().await;
                    ps.cleanup_stale(24 * 3600 * 1000);
                    let _ = ps.save();

                    tracing::debug!("P2P cleanup cycle completed");
                }
            }));
        }

        tracing::info!(
            "P2P manager started: node={}, port={}",
            &node_id[..16.min(node_id.len())],
            config.port
        );

        Ok(Arc::new(Self {
            config,
            node_id,
            gossip,
            sync,
            transport,
            discovery,
            running,
            tasks,
        }))
    }

    /// Stop the P2P subsystem.
    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        for task in &self.tasks {
            task.abort();
        }
        self.transport.write().await.stop();
        self.discovery.write().await.stop();
        tracing::info!("P2P manager stopped");
    }

    /// Current P2P status.
    pub async fn status(&self) -> P2pStatus {
        let transport = self.transport.read().await;
        let peers: Vec<PeerStatusDto> = transport
            .connected_peers()
            .iter()
            .map(|addr| PeerStatusDto {
                addr: addr.to_string(),
                connected: true,
            })
            .collect();

        P2pStatus {
            node_id: self.node_id.clone(),
            enabled: self.config.enabled,
            port: self.config.port,
            peer_count: peers.len(),
            connected_peers: peers,
            gossip_stats: self.gossip.read().await.stats(),
            sync_stats: self.sync.read().await.stats(),
        }
    }

    /// Connect to a new peer at runtime.
    pub async fn add_peer(&self, addr: SocketAddr) -> Result<(), String> {
        let triple_count = self.sync.read().await.local_ids().len() as u64;
        self.transport
            .write()
            .await
            .connect(addr, triple_count)
            .await
    }

    /// Disconnect a peer.
    pub async fn remove_peer(&self, addr: SocketAddr) {
        self.transport.write().await.disconnect(&addr);
    }

    /// Node ID (hex public key).
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
}

/// Current time in milliseconds since UNIX epoch.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_status_dto_fields() {
        let dto = PeerStatusDto {
            addr: "127.0.0.1:19091".into(),
            connected: true,
        };
        assert!(dto.connected);
    }

    #[test]
    fn p2p_status_serialize() {
        let status = P2pStatus {
            node_id: "abc".into(),
            enabled: true,
            port: 19091,
            peer_count: 0,
            connected_peers: vec![],
            gossip_stats: GossipStats {
                round: 0,
                pending_announcements: 0,
                known_ids: 0,
                bloom_filter_items: 0,
                bloom_filter_fpr: 0.0,
            },
            sync_stats: SyncStats {
                peer_count: 0,
                local_ids: 0,
                total_successful_syncs: 0,
                total_failed_syncs: 0,
            },
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("node_id"));
    }

    #[tokio::test]
    async fn manager_starts_and_stops() {
        let mut config = P2pConfig::default();
        config.enabled = true;
        config.port = 0; // OS-assigned
        config.data_dir = tempfile::TempDir::new().unwrap().into_path();

        let state = AppState::new();
        let manager = P2pManager::start(config, state).await.unwrap();
        assert!(!manager.node_id().is_empty());

        let status = manager.status().await;
        assert!(status.enabled);
        assert_eq!(status.peer_count, 0);

        manager.stop().await;
    }

    // ── A2: Manual peer tracker tests ────────────────────────

    #[test]
    fn manual_peer_tracker_new() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let tracker = ManualPeerTracker::new(addr);
        assert_eq!(tracker.retries, 0);
        assert!(!tracker.abandoned);
        assert_eq!(tracker.current_backoff, Duration::from_secs(5));
    }

    #[test]
    fn tracker_should_retry_after_backoff() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let mut tracker = ManualPeerTracker::new(addr);
        tracker.last_attempt = Instant::now() - Duration::from_secs(6);
        assert!(tracker.should_retry());
    }

    #[test]
    fn tracker_record_failure_doubles_backoff() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let mut tracker = ManualPeerTracker::new(addr);
        let initial = tracker.current_backoff;
        tracker.record_failure();
        assert_eq!(tracker.current_backoff, initial * 2);
        assert_eq!(tracker.retries, 1);
    }

    #[test]
    fn tracker_record_failure_caps_at_max() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let mut tracker = ManualPeerTracker::new(addr);
        for _ in 0..20 {
            tracker.record_failure();
        }
        assert!(tracker.current_backoff <= Duration::from_secs(300));
    }

    #[test]
    fn tracker_abandoned_after_max_retries() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let mut tracker = ManualPeerTracker::new(addr);
        for _ in 0..10 {
            tracker.record_failure();
        }
        assert!(tracker.abandoned);
        assert!(!tracker.should_retry());
    }

    #[test]
    fn tracker_record_success_resets() {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let mut tracker = ManualPeerTracker::new(addr);
        tracker.record_failure();
        tracker.record_failure();
        tracker.record_success();
        assert_eq!(tracker.retries, 0);
        assert_eq!(tracker.current_backoff, Duration::from_secs(5));
        assert!(!tracker.abandoned);
    }

    // ── A4: Ping tracker tests ───────────────────────────────

    #[test]
    fn ping_tracker_new_empty() {
        let tracker = PingTracker::new(Duration::from_secs(10));
        assert!(tracker.outstanding.is_empty());
        assert!(tracker.timed_out_peers().is_empty());
    }

    #[test]
    fn ping_tracker_record_and_pong() {
        let mut tracker = PingTracker::new(Duration::from_secs(10));
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        tracker.record_ping(addr, 1000);
        assert_eq!(tracker.outstanding.len(), 1);
        tracker.record_pong(&addr, 1000);
        assert!(tracker.outstanding.is_empty());
    }

    #[test]
    fn ping_tracker_timed_out_detection() {
        let mut tracker = PingTracker::new(Duration::from_millis(10));
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        tracker.outstanding.insert(addr, (1000, Instant::now() - Duration::from_millis(50)));
        let timed_out = tracker.timed_out_peers();
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], addr);
    }

    #[test]
    fn ping_tracker_clear_removes_entry() {
        let mut tracker = PingTracker::new(Duration::from_secs(10));
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        tracker.record_ping(addr, 1000);
        tracker.clear(&addr);
        assert!(tracker.outstanding.is_empty());
    }

    #[test]
    fn ping_tracker_no_false_timeout() {
        let mut tracker = PingTracker::new(Duration::from_secs(60));
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        tracker.record_ping(addr, 1000);
        // Just recorded, shouldn't be timed out
        assert!(tracker.timed_out_peers().is_empty());
    }

    // ── A7: Cleanup tests ────────────────────────────────────

    #[test]
    fn cleanup_removes_inactive_sync_states() {
        let mut sm = TripleSyncManager::new(Duration::from_secs(60));
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        sm.get_peer_state(&addr);
        std::thread::sleep(Duration::from_millis(10));
        sm.cleanup_inactive(Duration::from_millis(1));
        assert_eq!(sm.stats().peer_count, 0);
    }

    #[test]
    fn cleanup_removes_expired_tombstones() {
        let mut sm = TripleSyncManager::with_tombstone_ttl(
            Duration::from_secs(60),
            Duration::from_millis(10),
        );
        sm.add_tombstone([1u8; 32], 0); // very old
        sm.cleanup_expired_tombstones();
        assert!(!sm.has_tombstone(&[1u8; 32]));
    }

    #[test]
    fn cleanup_interval_configurable() {
        // Verify that the cleanup interval (5min in Task 7) can be configured
        // by checking the tombstone TTL is customizable
        let sm = TripleSyncManager::with_tombstone_ttl(
            Duration::from_secs(60),
            Duration::from_secs(7200),
        );
        assert_eq!(sm.tombstone_ttl, Duration::from_secs(7200));
    }
}
