//! WebRTC Transport for Browser Support
//!
//! This module enables AIngle nodes to run in web browsers and communicate
//! with native nodes through WebRTC data channels.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐     WebRTC     ┌─────────────────┐
//! │  Browser Node   │◄──────────────►│   Native Node   │
//! │  (JavaScript)   │   DataChannel  │    (Rust)       │
//! └─────────────────┘                └─────────────────┘
//!         │                                  │
//!         └──────────► Signaling ◄───────────┘
//!                      Server
//! ```
//!
//! # Features
//!
//! - **NAT Traversal**: Uses STUN/TURN for connectivity behind firewalls
//! - **Data Channels**: Reliable, ordered message delivery
//! - **Low Latency**: Direct peer-to-peer when possible
//! - **Browser Compatible**: Works with standard WebRTC APIs

use crate::error::{Error, Result};
use crate::network::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "webrtc")]
use async_tungstenite::tungstenite::Message as WsMessage;
#[cfg(feature = "webrtc")]
use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "webrtc")]
use smol::channel::{Receiver, Sender};
#[cfg(feature = "webrtc")]
use smol::lock::RwLock;
#[cfg(feature = "webrtc")]
use webrtc::api::APIBuilder;
#[cfg(feature = "webrtc")]
use webrtc::data_channel::RTCDataChannel;
#[cfg(feature = "webrtc")]
use webrtc::ice_transport::ice_server::RTCIceServer;
#[cfg(feature = "webrtc")]
use webrtc::peer_connection::configuration::RTCConfiguration;
#[cfg(feature = "webrtc")]
use webrtc::peer_connection::RTCPeerConnection;

/// Signaling message for WebRTC peer negotiation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalingMessage {
    /// SDP offer from initiator
    Offer {
        from: String,
        to: String,
        sdp: String,
    },
    /// SDP answer from responder
    Answer {
        from: String,
        to: String,
        sdp: String,
    },
    /// ICE candidate
    IceCandidate {
        from: String,
        to: String,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    },
    /// Peer joined the signaling room
    Join { peer_id: String },
    /// Peer left the signaling room
    Leave { peer_id: String },
}

/// Configuration for WebRTC transport
#[derive(Debug, Clone)]
pub struct WebRtcConfig {
    /// STUN server URL for NAT traversal
    pub stun_server: String,
    /// Optional TURN server for relay
    pub turn_server: Option<String>,
    /// TURN username (if using TURN)
    pub turn_username: Option<String>,
    /// TURN credential (if using TURN)
    pub turn_credential: Option<String>,
    /// Port for WebSocket signaling server
    pub signaling_port: u16,
    /// Maximum time to wait for ICE connection
    pub ice_timeout: Duration,
    /// Data channel label
    pub channel_label: String,
}

impl Default for WebRtcConfig {
    fn default() -> Self {
        Self {
            stun_server: "stun:stun.l.google.com:19302".to_string(),
            turn_server: None,
            turn_username: None,
            turn_credential: None,
            signaling_port: 8080,
            ice_timeout: Duration::from_secs(30),
            channel_label: "aingle".to_string(),
        }
    }
}

impl WebRtcConfig {
    /// Create a new WebRTC configuration with custom STUN server
    pub fn with_stun(stun_server: &str) -> Self {
        Self {
            stun_server: stun_server.to_string(),
            ..Default::default()
        }
    }

    /// Add TURN server for relay support
    pub fn with_turn(mut self, server: &str, username: &str, credential: &str) -> Self {
        self.turn_server = Some(server.to_string());
        self.turn_username = Some(username.to_string());
        self.turn_credential = Some(credential.to_string());
        self
    }
}

/// Peer connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state, no connection attempt yet
    New,
    /// ICE gathering in progress
    Connecting,
    /// Connection established and ready
    Connected,
    /// Connection temporarily interrupted
    Disconnected,
    /// Connection failed permanently
    Failed,
    /// Connection closed by either party
    Closed,
}

/// Statistics for a WebRTC connection
#[derive(Debug, Clone, Default)]
pub struct WebRtcStats {
    /// Number of messages sent
    pub messages_sent: u64,
    /// Number of messages received
    pub messages_received: u64,
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Round-trip time in milliseconds
    pub rtt_ms: u32,
    /// Number of ICE candidates gathered
    pub ice_candidates: u32,
    /// Whether TURN relay is being used
    pub using_relay: bool,
}

/// A WebRTC peer connection
#[derive(Debug)]
pub struct PeerConnection {
    /// Unique peer identifier
    pub peer_id: String,
    /// Connection state
    pub state: ConnectionState,
    /// Remote address (if known)
    pub remote_addr: Option<SocketAddr>,
    /// Connection statistics
    pub stats: WebRtcStats,
    /// Time when connection was established
    pub connected_at: Option<Instant>,
}

impl PeerConnection {
    /// Create a new peer connection
    pub fn new(peer_id: &str) -> Self {
        Self {
            peer_id: peer_id.to_string(),
            state: ConnectionState::New,
            remote_addr: None,
            stats: WebRtcStats::default(),
            connected_at: None,
        }
    }

    /// Check if connection is active
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Get connection duration
    pub fn connection_duration(&self) -> Option<Duration> {
        self.connected_at.map(|t| t.elapsed())
    }
}

/// WebRTC transport server
///
/// Manages WebRTC peer connections and provides signaling coordination.
pub struct WebRtcServer {
    /// Server configuration
    config: WebRtcConfig,
    /// Active peer connections
    peers: HashMap<String, PeerConnection>,
    /// Running state
    running: bool,
    /// Local peer ID
    local_peer_id: String,
    /// WebRTC API instance
    #[cfg(feature = "webrtc")]
    api: Option<webrtc::api::API>,
    /// RTCPeerConnections indexed by peer ID
    #[cfg(feature = "webrtc")]
    rtc_peers: HashMap<String, Arc<RTCPeerConnection>>,
    /// Data channels indexed by peer ID
    #[cfg(feature = "webrtc")]
    data_channels: HashMap<String, Arc<RTCDataChannel>>,
    /// Channel for receiving messages from data channels
    #[cfg(feature = "webrtc")]
    message_rx: Option<Receiver<(String, Vec<u8>)>>,
    /// Channel for sending messages (used by data channel callbacks)
    #[cfg(feature = "webrtc")]
    message_tx: Option<Sender<(String, Vec<u8>)>>,
    /// Pending signaling messages to send
    #[cfg(feature = "webrtc")]
    signaling_queue: Vec<SignalingMessage>,
}

impl WebRtcServer {
    /// Create a new WebRTC server
    pub fn new(config: WebRtcConfig) -> Self {
        let local_peer_id = Self::generate_peer_id();
        Self {
            config,
            peers: HashMap::new(),
            running: false,
            local_peer_id,
            #[cfg(feature = "webrtc")]
            api: None,
            #[cfg(feature = "webrtc")]
            rtc_peers: HashMap::new(),
            #[cfg(feature = "webrtc")]
            data_channels: HashMap::new(),
            #[cfg(feature = "webrtc")]
            message_rx: None,
            #[cfg(feature = "webrtc")]
            message_tx: None,
            #[cfg(feature = "webrtc")]
            signaling_queue: Vec::new(),
        }
    }

    /// Generate a unique peer ID
    fn generate_peer_id() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.gen();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Get local peer ID
    pub fn local_peer_id(&self) -> &str {
        &self.local_peer_id
    }

    /// Start the WebRTC server
    pub async fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }

        log::info!(
            "Starting WebRTC server on signaling port {}",
            self.config.signaling_port
        );

        #[cfg(feature = "webrtc")]
        {
            // Initialize WebRTC API
            let api = APIBuilder::new().build();
            self.api = Some(api);

            // Create message channels for data channel communication
            let (tx, rx) = smol::channel::unbounded();
            self.message_tx = Some(tx);
            self.message_rx = Some(rx);

            log::info!("WebRTC API initialized");
        }

        #[cfg(not(feature = "webrtc"))]
        {
            log::warn!("WebRTC feature not enabled, using simulated mode");
        }

        self.running = true;
        Ok(())
    }

    /// Stop the WebRTC server
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }

        log::info!("Stopping WebRTC server");

        #[cfg(feature = "webrtc")]
        {
            // Close all RTCPeerConnections
            for (peer_id, pc) in self.rtc_peers.drain() {
                if let Err(e) = pc.close().await {
                    log::warn!("Error closing peer connection {}: {}", peer_id, e);
                }
            }

            // Clear data channels
            self.data_channels.clear();

            // Drop API
            self.api = None;
        }

        // Close all peer connections
        for (peer_id, mut peer) in self.peers.drain() {
            peer.state = ConnectionState::Closed;
            log::debug!("Closed connection to peer: {}", peer_id);
        }

        self.running = false;
        Ok(())
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Connect to a peer
    pub async fn connect(&mut self, peer_id: &str) -> Result<()> {
        if self.peers.contains_key(peer_id) {
            return Err(Error::Network(format!(
                "Already connected to peer: {}",
                peer_id
            )));
        }

        let mut peer = PeerConnection::new(peer_id);
        peer.state = ConnectionState::Connecting;

        log::info!("Initiating WebRTC connection to peer: {}", peer_id);

        #[cfg(feature = "webrtc")]
        {
            let api = self
                .api
                .as_ref()
                .ok_or_else(|| Error::Network("WebRTC API not initialized".to_string()))?;

            // Configure ICE servers
            let mut ice_servers = vec![RTCIceServer {
                urls: vec![self.config.stun_server.clone()],
                ..Default::default()
            }];

            // Add TURN server if configured
            if let Some(turn) = &self.config.turn_server {
                ice_servers.push(RTCIceServer {
                    urls: vec![turn.clone()],
                    username: self.config.turn_username.clone().unwrap_or_default(),
                    credential: self.config.turn_credential.clone().unwrap_or_default(),
                    ..Default::default()
                });
            }

            let rtc_config = RTCConfiguration {
                ice_servers,
                ..Default::default()
            };

            // Create new RTCPeerConnection
            let pc = api
                .new_peer_connection(rtc_config)
                .await
                .map_err(|e| Error::Network(format!("Failed to create peer connection: {}", e)))?;

            // Create data channel
            let dc = pc
                .create_data_channel(&self.config.channel_label, None)
                .await
                .map_err(|e| Error::Network(format!("Failed to create data channel: {}", e)))?;

            log::debug!(
                "Created data channel '{}' for peer {}",
                self.config.channel_label,
                peer_id
            );

            // Store the peer connection and data channel
            let pc = Arc::new(pc);
            self.rtc_peers.insert(peer_id.to_string(), pc);
            self.data_channels.insert(peer_id.to_string(), dc);

            // Generate offer SDP
            // Note: In a real implementation, you would:
            // 1. Create offer: pc.create_offer(None).await
            // 2. Set local description: pc.set_local_description(offer).await
            // 3. Send offer via signaling server
            // 4. Receive answer via signaling
            // 5. Set remote description: pc.set_remote_description(answer).await
            // 6. Exchange ICE candidates via signaling

            log::info!("WebRTC peer connection created for: {}", peer_id);
        }

        self.peers.insert(peer_id.to_string(), peer);
        Ok(())
    }

    /// Disconnect from a peer
    pub async fn disconnect(&mut self, peer_id: &str) -> Result<()> {
        #[cfg(feature = "webrtc")]
        {
            // Close and remove the RTCPeerConnection
            if let Some(pc) = self.rtc_peers.remove(peer_id) {
                if let Err(e) = pc.close().await {
                    log::warn!("Error closing peer connection {}: {}", peer_id, e);
                }
            }

            // Remove data channel
            self.data_channels.remove(peer_id);
        }

        if let Some(mut peer) = self.peers.remove(peer_id) {
            peer.state = ConnectionState::Closed;
            log::info!("Disconnected from peer: {}", peer_id);
            Ok(())
        } else {
            Err(Error::Network(format!("Peer not found: {}", peer_id)))
        }
    }

    /// Send a message to a peer
    pub async fn send(&mut self, peer_id: &str, message: &Message) -> Result<()> {
        let peer = self
            .peers
            .get_mut(peer_id)
            .ok_or_else(|| Error::Network(format!("Peer not found: {}", peer_id)))?;

        if peer.state != ConnectionState::Connected {
            return Err(Error::Network(format!("Peer not connected: {}", peer_id)));
        }

        let payload =
            serde_json::to_vec(message).map_err(|e| Error::Serialization(e.to_string()))?;

        #[cfg(feature = "webrtc")]
        {
            let dc = self
                .data_channels
                .get(peer_id)
                .ok_or_else(|| Error::Network(format!("Data channel not found: {}", peer_id)))?;

            // Send via data channel
            dc.send(&bytes::Bytes::from(payload.clone()))
                .await
                .map_err(|e| Error::Network(format!("Failed to send: {}", e)))?;
        }

        peer.stats.messages_sent += 1;
        peer.stats.bytes_sent += payload.len() as u64;

        log::debug!("Sent message to peer {} ({} bytes)", peer_id, payload.len());
        Ok(())
    }

    /// Receive messages from all peers
    pub async fn recv(&mut self) -> Result<Option<(String, Message)>> {
        #[cfg(feature = "webrtc")]
        {
            if let Some(ref rx) = self.message_rx {
                // Try to receive without blocking
                match rx.try_recv() {
                    Ok((peer_id, data)) => {
                        let message: Message = serde_json::from_slice(&data)
                            .map_err(|e| Error::Serialization(e.to_string()))?;

                        // Update peer stats
                        if let Some(peer) = self.peers.get_mut(&peer_id) {
                            peer.stats.messages_received += 1;
                            peer.stats.bytes_received += data.len() as u64;
                        }

                        log::debug!(
                            "Received message from peer {} ({} bytes)",
                            peer_id,
                            data.len()
                        );

                        return Ok(Some((peer_id, message)));
                    }
                    Err(smol::channel::TryRecvError::Empty) => {
                        return Ok(None);
                    }
                    Err(smol::channel::TryRecvError::Closed) => {
                        return Err(Error::Network("Message channel closed".to_string()));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Get connected peer count
    pub fn peer_count(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.state == ConnectionState::Connected)
            .count()
    }

    /// Get all peer IDs
    pub fn peer_ids(&self) -> Vec<String> {
        self.peers.keys().cloned().collect()
    }

    /// Get peer connection info
    pub fn get_peer(&self, peer_id: &str) -> Option<&PeerConnection> {
        self.peers.get(peer_id)
    }

    /// Get aggregate statistics
    pub fn stats(&self) -> WebRtcStats {
        let mut stats = WebRtcStats::default();
        for peer in self.peers.values() {
            stats.messages_sent += peer.stats.messages_sent;
            stats.messages_received += peer.stats.messages_received;
            stats.bytes_sent += peer.stats.bytes_sent;
            stats.bytes_received += peer.stats.bytes_received;
        }
        stats
    }

    /// Queue a signaling message to be sent
    #[cfg(feature = "webrtc")]
    pub fn queue_signaling(&mut self, message: SignalingMessage) {
        self.signaling_queue.push(message);
    }

    /// Get pending signaling messages and clear the queue
    #[cfg(feature = "webrtc")]
    pub fn drain_signaling_queue(&mut self) -> Vec<SignalingMessage> {
        std::mem::take(&mut self.signaling_queue)
    }

    /// Process an incoming signaling message
    #[cfg(feature = "webrtc")]
    pub async fn handle_signaling(&mut self, message: SignalingMessage) -> Result<()> {
        match message {
            SignalingMessage::Offer { from, to, sdp } => {
                if to == self.local_peer_id {
                    log::debug!("Received offer from {}: {} bytes", from, sdp.len());
                    // In a real implementation:
                    // 1. Create RTCPeerConnection for this peer
                    // 2. Set remote description from offer SDP
                    // 3. Create answer
                    // 4. Set local description
                    // 5. Queue answer for sending
                }
            }
            SignalingMessage::Answer { from, to, sdp } => {
                if to == self.local_peer_id {
                    log::debug!("Received answer from {}: {} bytes", from, sdp.len());
                    // In a real implementation:
                    // Set remote description from answer SDP
                }
            }
            SignalingMessage::IceCandidate {
                from,
                to,
                candidate,
                sdp_mid,
                sdp_mline_index,
            } => {
                if to == self.local_peer_id {
                    log::debug!(
                        "Received ICE candidate from {}: mid={:?}, index={:?}",
                        from,
                        sdp_mid,
                        sdp_mline_index
                    );
                    // In a real implementation:
                    // Add ICE candidate to the peer connection
                    let _ = candidate; // suppress warning
                }
            }
            SignalingMessage::Join { peer_id } => {
                log::info!("Peer joined: {}", peer_id);
            }
            SignalingMessage::Leave { peer_id } => {
                log::info!("Peer left: {}", peer_id);
                // Disconnect if we were connected
                let _ = self.disconnect(&peer_id).await;
            }
        }
        Ok(())
    }
}

// ============================================================================
// Signaling Server Implementation
// ============================================================================

/// Configuration for the WebSocket signaling server
#[derive(Debug, Clone)]
pub struct SignalingConfig {
    /// Address to bind the signaling server
    pub bind_addr: String,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Heartbeat interval for keepalive
    pub heartbeat_interval: Duration,
    /// Connection timeout
    pub connection_timeout: Duration,
}

impl Default for SignalingConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".to_string(),
            max_connections: 100,
            heartbeat_interval: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(60),
        }
    }
}

/// A connected client on the signaling server
#[cfg(feature = "webrtc")]
#[derive(Debug)]
#[allow(dead_code)] // peer_id used for debugging
struct ConnectedPeer {
    /// Peer ID of the client
    peer_id: String,
    /// Channel to send messages to this client
    tx: Sender<SignalingMessage>,
    /// Time of last activity
    last_activity: Instant,
}

/// WebSocket-based signaling server for WebRTC peer discovery and SDP exchange
///
/// The signaling server facilitates:
/// - Peer discovery: Clients can find each other
/// - SDP exchange: Offers and answers are relayed between peers
/// - ICE candidate exchange: NAT traversal information is shared
///
/// # Example
///
/// ```rust,ignore
/// use aingle_minimal::{SignalingServer, SignalingConfig};
///
/// let config = SignalingConfig::default();
/// let mut server = SignalingServer::new(config);
///
/// // Start the signaling server
/// smol::block_on(async {
///     server.start().await.unwrap();
/// });
/// ```
#[cfg(feature = "webrtc")]
pub struct SignalingServer {
    /// Server configuration
    config: SignalingConfig,
    /// Connected clients indexed by peer_id
    clients: Arc<RwLock<HashMap<String, ConnectedPeer>>>,
    /// Running state
    running: Arc<std::sync::atomic::AtomicBool>,
    /// Server task handle
    server_task: Option<smol::Task<()>>,
}

#[cfg(feature = "webrtc")]
impl SignalingServer {
    /// Create a new signaling server
    pub fn new(config: SignalingConfig) -> Self {
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            server_task: None,
        }
    }

    /// Start the signaling server
    pub async fn start(&mut self) -> Result<()> {
        if self.running.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }

        let addr = &self.config.bind_addr;
        log::info!("Starting signaling server on {}", addr);

        let listener = smol::net::TcpListener::bind(addr)
            .await
            .map_err(|e| Error::Network(format!("Failed to bind signaling server: {}", e)))?;

        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let clients = self.clients.clone();
        let running = self.running.clone();
        let max_connections = self.config.max_connections;

        let task = smol::spawn(async move {
            log::info!("Signaling server listening for connections");

            while running.load(std::sync::atomic::Ordering::SeqCst) {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        // Check connection limit
                        let client_count = clients.read().await.len();
                        if client_count >= max_connections {
                            log::warn!(
                                "Max connections reached ({}), rejecting {}",
                                max_connections,
                                addr
                            );
                            continue;
                        }

                        log::debug!("New signaling connection from {}", addr);

                        let clients = clients.clone();

                        // Handle this connection in a separate task
                        smol::spawn(async move {
                            if let Err(e) = Self::handle_connection(stream, addr, clients).await {
                                log::warn!("Connection error from {}: {}", addr, e);
                            }
                        })
                        .detach();
                    }
                    Err(e) => {
                        log::error!("Accept error: {}", e);
                    }
                }
            }

            log::info!("Signaling server stopped");
        });

        self.server_task = Some(task);
        Ok(())
    }

    /// Handle a single WebSocket connection
    async fn handle_connection(
        stream: smol::net::TcpStream,
        addr: SocketAddr,
        clients: Arc<RwLock<HashMap<String, ConnectedPeer>>>,
    ) -> Result<()> {
        // Upgrade to WebSocket
        let ws_stream = async_tungstenite::accept_async(stream)
            .await
            .map_err(|e| Error::Network(format!("WebSocket upgrade failed: {}", e)))?;

        let (mut ws_sink, mut ws_stream) = ws_stream.split();

        // Wait for Join message to get peer ID
        let peer_id = loop {
            match ws_stream.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    match serde_json::from_str::<SignalingMessage>(&text) {
                        Ok(SignalingMessage::Join { peer_id }) => break peer_id,
                        Ok(_) => {
                            log::warn!("Expected Join message from {}", addr);
                        }
                        Err(e) => {
                            log::warn!("Invalid message from {}: {}", addr, e);
                        }
                    }
                }
                Some(Ok(WsMessage::Close(_))) | None => {
                    return Ok(());
                }
                _ => continue,
            }
        };

        log::info!("Peer '{}' joined from {}", peer_id, addr);

        // Create channels for this client
        let (tx, rx) = smol::channel::bounded(32);

        // Register client
        {
            let mut clients_guard = clients.write().await;
            clients_guard.insert(
                peer_id.clone(),
                ConnectedPeer {
                    peer_id: peer_id.clone(),
                    tx: tx.clone(),
                    last_activity: Instant::now(),
                },
            );

            // Notify other clients about the new peer
            let join_msg = SignalingMessage::Join {
                peer_id: peer_id.clone(),
            };
            for (id, client) in clients_guard.iter() {
                if id != &peer_id {
                    let _ = client.tx.try_send(join_msg.clone());
                }
            }
        }

        // Spawn task to forward messages from channel to WebSocket
        let forward_task = smol::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                let json = match serde_json::to_string(&msg) {
                    Ok(j) => j,
                    Err(e) => {
                        log::warn!("Failed to serialize message: {}", e);
                        continue;
                    }
                };
                if ws_sink.send(WsMessage::Text(json)).await.is_err() {
                    break;
                }
            }
        });

        // Process incoming messages
        while let Some(msg_result) = ws_stream.next().await {
            match msg_result {
                Ok(WsMessage::Text(text)) => {
                    match serde_json::from_str::<SignalingMessage>(&text) {
                        Ok(msg) => {
                            Self::route_message(&clients, &peer_id, msg).await;
                        }
                        Err(e) => {
                            log::warn!("Invalid message from {}: {}", peer_id, e);
                        }
                    }
                }
                Ok(WsMessage::Ping(data)) => {
                    // Update activity timestamp
                    if let Some(client) = clients.write().await.get_mut(&peer_id) {
                        client.last_activity = Instant::now();
                    }
                    // Pong is handled automatically by tungstenite
                    let _ = data; // suppress warning
                }
                Ok(WsMessage::Close(_)) => {
                    log::info!("Peer '{}' disconnected", peer_id);
                    break;
                }
                Err(e) => {
                    log::warn!("WebSocket error from {}: {}", peer_id, e);
                    break;
                }
                _ => {}
            }
        }

        // Clean up
        forward_task.cancel().await;

        // Remove client and notify others
        {
            let mut clients_guard = clients.write().await;
            clients_guard.remove(&peer_id);

            let leave_msg = SignalingMessage::Leave {
                peer_id: peer_id.clone(),
            };
            for client in clients_guard.values() {
                let _ = client.tx.try_send(leave_msg.clone());
            }
        }

        log::info!("Peer '{}' cleanup complete", peer_id);
        Ok(())
    }

    /// Route a signaling message to its destination
    async fn route_message(
        clients: &Arc<RwLock<HashMap<String, ConnectedPeer>>>,
        from: &str,
        message: SignalingMessage,
    ) {
        let target_peer_id = match &message {
            SignalingMessage::Offer { to, .. } => Some(to.clone()),
            SignalingMessage::Answer { to, .. } => Some(to.clone()),
            SignalingMessage::IceCandidate { to, .. } => Some(to.clone()),
            SignalingMessage::Join { .. } | SignalingMessage::Leave { .. } => None,
        };

        if let Some(target) = target_peer_id {
            let clients_guard = clients.read().await;
            if let Some(client) = clients_guard.get(&target) {
                if let Err(e) = client.tx.try_send(message) {
                    log::warn!("Failed to route message from {} to {}: {}", from, target, e);
                }
            } else {
                log::warn!(
                    "Target peer '{}' not found for message from '{}'",
                    target,
                    from
                );
            }
        }
    }

    /// Stop the signaling server
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }

        log::info!("Stopping signaling server");
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // Cancel server task
        if let Some(task) = self.server_task.take() {
            task.cancel().await;
        }

        // Clear all clients
        self.clients.write().await.clear();

        Ok(())
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get number of connected clients
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Get list of connected peer IDs
    pub async fn peer_ids(&self) -> Vec<String> {
        self.clients.read().await.keys().cloned().collect()
    }
}

/// Signaling client for connecting to a signaling server
///
/// Used by WebRTC peers to connect to a signaling server for peer discovery
/// and SDP/ICE exchange.
///
/// # Example
///
/// ```rust,ignore
/// use aingle_minimal::SignalingClient;
///
/// let mut client = SignalingClient::new("ws://localhost:8080", "my-peer-id");
///
/// smol::block_on(async {
///     client.connect().await.unwrap();
///
///     // Receive signaling messages
///     while let Some(msg) = client.recv().await.unwrap() {
///         println!("Received: {:?}", msg);
///     }
/// });
/// ```
#[cfg(feature = "webrtc")]
pub struct SignalingClient {
    /// Server URL
    server_url: String,
    /// Local peer ID
    peer_id: String,
    /// Sender for outgoing messages
    tx: Option<Sender<SignalingMessage>>,
    /// Receiver for incoming messages
    rx: Option<Receiver<SignalingMessage>>,
    /// Connected state
    connected: bool,
}

#[cfg(feature = "webrtc")]
impl SignalingClient {
    /// Create a new signaling client
    pub fn new(server_url: &str, peer_id: &str) -> Self {
        Self {
            server_url: server_url.to_string(),
            peer_id: peer_id.to_string(),
            tx: None,
            rx: None,
            connected: false,
        }
    }

    /// Connect to the signaling server
    pub async fn connect(&mut self) -> Result<()> {
        if self.connected {
            return Ok(());
        }

        log::info!("Connecting to signaling server: {}", self.server_url);

        let (ws_stream, _) = async_tungstenite::async_std::connect_async(&self.server_url)
            .await
            .map_err(|e| Error::Network(format!("Failed to connect to signaling server: {}", e)))?;

        let (mut ws_sink, mut ws_stream) = ws_stream.split();

        // Send Join message
        let join_msg = SignalingMessage::Join {
            peer_id: self.peer_id.clone(),
        };
        let json =
            serde_json::to_string(&join_msg).map_err(|e| Error::Serialization(e.to_string()))?;
        ws_sink
            .send(WsMessage::Text(json))
            .await
            .map_err(|e| Error::Network(format!("Failed to send Join: {}", e)))?;

        // Create channels
        let (out_tx, out_rx) = smol::channel::bounded::<SignalingMessage>(32);
        let (in_tx, in_rx) = smol::channel::bounded::<SignalingMessage>(32);

        self.tx = Some(out_tx);
        self.rx = Some(in_rx);

        // Spawn task to forward outgoing messages
        smol::spawn(async move {
            while let Ok(msg) = out_rx.recv().await {
                let json = match serde_json::to_string(&msg) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                if ws_sink.send(WsMessage::Text(json)).await.is_err() {
                    break;
                }
            }
        })
        .detach();

        // Spawn task to receive incoming messages
        smol::spawn(async move {
            while let Some(Ok(msg)) = ws_stream.next().await {
                if let WsMessage::Text(text) = msg {
                    if let Ok(signaling_msg) = serde_json::from_str::<SignalingMessage>(&text) {
                        if in_tx.send(signaling_msg).await.is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .detach();

        self.connected = true;
        log::info!("Connected to signaling server as '{}'", self.peer_id);

        Ok(())
    }

    /// Send a signaling message
    pub async fn send(&self, message: SignalingMessage) -> Result<()> {
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| Error::Network("Not connected".to_string()))?;

        tx.send(message)
            .await
            .map_err(|e| Error::Network(format!("Failed to send: {}", e)))
    }

    /// Receive a signaling message (non-blocking)
    pub async fn recv(&self) -> Result<Option<SignalingMessage>> {
        let rx = self
            .rx
            .as_ref()
            .ok_or_else(|| Error::Network("Not connected".to_string()))?;

        match rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(smol::channel::TryRecvError::Empty) => Ok(None),
            Err(smol::channel::TryRecvError::Closed) => {
                Err(Error::Network("Channel closed".to_string()))
            }
        }
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Get local peer ID
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webrtc_config_default() {
        let config = WebRtcConfig::default();
        assert!(config.stun_server.contains("stun"));
        assert!(config.turn_server.is_none());
        assert_eq!(config.signaling_port, 8080);
    }

    #[test]
    fn test_webrtc_config_with_stun() {
        let config = WebRtcConfig::with_stun("stun:custom.server:3478");
        assert_eq!(config.stun_server, "stun:custom.server:3478");
    }

    #[test]
    fn test_webrtc_config_with_turn() {
        let config = WebRtcConfig::default().with_turn("turn:relay.server:3478", "user", "pass");
        assert!(config.turn_server.is_some());
        assert_eq!(config.turn_username, Some("user".to_string()));
    }

    #[test]
    fn test_peer_connection_new() {
        let peer = PeerConnection::new("test-peer");
        assert_eq!(peer.peer_id, "test-peer");
        assert_eq!(peer.state, ConnectionState::New);
        assert!(!peer.is_connected());
    }

    #[test]
    fn test_peer_connection_connected() {
        let mut peer = PeerConnection::new("test-peer");
        peer.state = ConnectionState::Connected;
        peer.connected_at = Some(Instant::now());
        assert!(peer.is_connected());
        assert!(peer.connection_duration().is_some());
    }

    #[test]
    fn test_webrtc_server_creation() {
        let config = WebRtcConfig::default();
        let server = WebRtcServer::new(config);
        assert!(!server.is_running());
        assert_eq!(server.peer_count(), 0);
        assert!(!server.local_peer_id().is_empty());
    }

    #[test]
    fn test_connection_state_equality() {
        assert_eq!(ConnectionState::New, ConnectionState::New);
        assert_ne!(ConnectionState::Connected, ConnectionState::Disconnected);
    }

    #[test]
    fn test_webrtc_stats_default() {
        let stats = WebRtcStats::default();
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.bytes_sent, 0);
        assert!(!stats.using_relay);
    }

    #[test]
    fn test_signaling_config_default() {
        let config = SignalingConfig::default();
        assert_eq!(config.bind_addr, "0.0.0.0:8080");
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
    }

    #[test]
    fn test_signaling_message_serialization() {
        let offer = SignalingMessage::Offer {
            from: "peer-a".to_string(),
            to: "peer-b".to_string(),
            sdp: "v=0\r\n...".to_string(),
        };

        let json = serde_json::to_string(&offer).unwrap();
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();

        if let SignalingMessage::Offer { from, to, sdp } = parsed {
            assert_eq!(from, "peer-a");
            assert_eq!(to, "peer-b");
            assert!(sdp.contains("v=0"));
        } else {
            panic!("Expected Offer message");
        }
    }

    #[test]
    fn test_signaling_message_ice_candidate() {
        let ice = SignalingMessage::IceCandidate {
            from: "peer-a".to_string(),
            to: "peer-b".to_string(),
            candidate: "candidate:1 1 UDP 2130706431 192.168.1.1 54321 typ host".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
        };

        let json = serde_json::to_string(&ice).unwrap();
        assert!(json.contains("IceCandidate"));
        assert!(json.contains("candidate:1"));
    }

    #[test]
    fn test_signaling_message_join_leave() {
        let join = SignalingMessage::Join {
            peer_id: "test-peer".to_string(),
        };
        let json = serde_json::to_string(&join).unwrap();
        assert!(json.contains("test-peer"));

        let leave = SignalingMessage::Leave {
            peer_id: "test-peer".to_string(),
        };
        let json = serde_json::to_string(&leave).unwrap();
        assert!(json.contains("Leave"));
    }
}
