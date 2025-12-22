//! CoAP Transport for IoT nodes
//!
//! Implements the Constrained Application Protocol (RFC 7252) for
//! lightweight IoT communication over UDP.
//!
//! # Features
//! - Confirmable (CON) and Non-confirmable (NON) messages
//! - Block-wise transfer for large payloads (RFC 7959)
//! - Multicast discovery
//! - Resource-based routing
//!
//! # Resources
//! - `/.well-known/core` - CoRE Link Format discovery
//! - `/gossip` - Gossip protocol messages
//! - `/record` - Record retrieval
//! - `/announce` - New record announcements
//! - `/ping` - Liveness checks

use coap_lite::{CoapOption, MessageClass, MessageType, Packet, RequestType, ResponseType};

use crate::error::{Error, Result};
use crate::network::Message;
use async_io::Async;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// Maximum CoAP message size (for non-block transfers)
pub const COAP_MAX_MESSAGE_SIZE: usize = 1024;

/// Default CoAP port
pub const COAP_DEFAULT_PORT: u16 = 5683;

/// CoAP multicast address for discovery (IPv4)
pub const COAP_MULTICAST_IPV4: &str = "224.0.1.187";

/// CoAP multicast address for discovery (IPv6)
#[allow(dead_code)]
pub const COAP_MULTICAST_IPV6: &str = "ff02::fd";

/// Block size for block-wise transfers (256 bytes = SZX 2)
pub const BLOCK_SIZE: usize = 256;

/// Maximum retransmissions for CON messages
pub const MAX_RETRANSMIT: u8 = 4;

/// ACK timeout in milliseconds
pub const ACK_TIMEOUT_MS: u64 = 2000;

/// CoAP message token for tracking requests
pub type Token = Vec<u8>;

/// Pending request tracking
#[derive(Debug)]
struct PendingRequest {
    /// Original request packet
    packet: Packet,
    /// Destination address
    addr: SocketAddr,
    /// Time sent
    sent_at: Instant,
    /// Retransmission count
    retransmits: u8,
}

/// Block transfer state
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct BlockState {
    /// Full data being transferred
    data: Vec<u8>,
    /// Current block number
    block_num: u32,
    /// More blocks flag
    more: bool,
}

/// CoAP Server for handling incoming requests
pub struct CoapServer {
    /// UDP socket (async)
    socket: Option<Async<UdpSocket>>,
    /// Bind address
    bind_addr: String,
    /// Port
    port: u16,
    /// Node ID for identification
    #[allow(dead_code)]
    node_id: String,
    /// Message ID counter
    message_id: u16,
    /// Pending requests awaiting ACK
    pending_requests: HashMap<u16, PendingRequest>,
    /// Block transfer states (for future block-wise receive)
    #[allow(dead_code)]
    block_states: HashMap<Token, BlockState>,
    /// Running flag
    running: bool,
}

impl CoapServer {
    /// Create a new CoAP server
    pub fn new(bind_addr: String, port: u16, node_id: String) -> Self {
        Self {
            socket: None,
            bind_addr,
            port,
            node_id,
            message_id: rand::random(),
            pending_requests: HashMap::new(),
            block_states: HashMap::new(),
            running: false,
        }
    }

    /// Start the CoAP server
    pub async fn start(&mut self) -> Result<()> {
        let addr = format!("{}:{}", self.bind_addr, self.port);
        let socket = UdpSocket::bind(&addr)
            .map_err(|e| Error::Network(format!("Failed to bind CoAP socket: {}", e)))?;

        // Set non-blocking for async
        socket
            .set_nonblocking(true)
            .map_err(|e| Error::Network(format!("Failed to set non-blocking: {}", e)))?;

        let async_socket = Async::new(socket)
            .map_err(|e| Error::Network(format!("Failed to create async socket: {}", e)))?;

        self.socket = Some(async_socket);
        self.running = true;

        log::info!("CoAP server started on {}:{}", self.bind_addr, self.port);
        Ok(())
    }

    /// Stop the CoAP server
    pub async fn stop(&mut self) -> Result<()> {
        self.running = false;
        self.socket = None;
        log::info!("CoAP server stopped");
        Ok(())
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.running && self.socket.is_some()
    }

    /// Receive and process a single message
    pub async fn recv(&self) -> Result<Option<(SocketAddr, Message)>> {
        let socket = self
            .socket
            .as_ref()
            .ok_or_else(|| Error::Network("Socket not initialized".to_string()))?;

        let mut buf = [0u8; 2048];

        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => {
                let packet = Packet::from_bytes(&buf[..len])
                    .map_err(|e| Error::Network(format!("Failed to parse CoAP packet: {:?}", e)))?;

                // Process the packet and convert to Message
                if let Some(msg) = self.process_packet(&packet, &addr)? {
                    return Ok(Some((addr, msg)));
                }
                Ok(None)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(Error::Network(format!("Receive error: {}", e))),
        }
    }

    /// Process incoming CoAP packet
    fn process_packet(&self, packet: &Packet, addr: &SocketAddr) -> Result<Option<Message>> {
        let path = Self::extract_path(packet);

        log::debug!(
            "CoAP request from {}: {} {:?}",
            addr,
            path,
            packet.header.code
        );

        match path.as_str() {
            "/.well-known/core" => {
                // Discovery - return available resources
                Ok(None) // Handled separately
            }
            "/ping" => {
                // Liveness check
                if !packet.payload.is_empty() {
                    let node_id = String::from_utf8_lossy(&packet.payload).to_string();
                    Ok(Some(Message::Ping { node_id }))
                } else {
                    Ok(Some(Message::Ping {
                        node_id: "unknown".to_string(),
                    }))
                }
            }
            "/gossip" => {
                // Gossip request
                if !packet.payload.is_empty() {
                    let msg: Message = serde_json::from_slice(&packet.payload)
                        .map_err(|e| Error::Serialization(e.to_string()))?;
                    Ok(Some(msg))
                } else {
                    Ok(None)
                }
            }
            "/record" => {
                // Record request
                if !packet.payload.is_empty() {
                    let msg: Message = serde_json::from_slice(&packet.payload)
                        .map_err(|e| Error::Serialization(e.to_string()))?;
                    Ok(Some(msg))
                } else {
                    Ok(None)
                }
            }
            "/announce" => {
                // New record announcement
                if !packet.payload.is_empty() {
                    let msg: Message = serde_json::from_slice(&packet.payload)
                        .map_err(|e| Error::Serialization(e.to_string()))?;
                    Ok(Some(msg))
                } else {
                    Ok(None)
                }
            }
            _ => {
                log::warn!("Unknown CoAP path: {}", path);
                Ok(None)
            }
        }
    }

    /// Extract URI path from CoAP packet
    fn extract_path(packet: &Packet) -> String {
        let mut path = String::from("/");

        // Get UriPath options and build path
        if let Some(uri_paths) = packet.get_option(CoapOption::UriPath) {
            for segment in uri_paths {
                if path.len() > 1 {
                    path.push('/');
                }
                path.push_str(&String::from_utf8_lossy(segment));
            }
        }

        if path == "/" {
            path = "/.well-known/core".to_string();
        }
        path
    }

    /// Send a CoAP message
    pub async fn send(
        &mut self,
        addr: &SocketAddr,
        message: &Message,
        confirmable: bool,
    ) -> Result<()> {
        let payload =
            serde_json::to_vec(message).map_err(|e| Error::Serialization(e.to_string()))?;

        // Determine the path based on message type
        let path = Self::message_to_path(message);

        // Check if we need block-wise transfer
        if payload.len() > COAP_MAX_MESSAGE_SIZE {
            return self.send_blocks(addr, &path, &payload, confirmable).await;
        }

        let packet = self.create_request_packet(&path, &payload, confirmable);
        let bytes = packet
            .to_bytes()
            .map_err(|e| Error::Network(format!("Failed to serialize packet: {:?}", e)))?;

        {
            let socket = self
                .socket
                .as_ref()
                .ok_or_else(|| Error::Network("Socket not initialized".to_string()))?;
            socket
                .send_to(&bytes, *addr)
                .await
                .map_err(|e| Error::Network(format!("Send error: {}", e)))?;
        }

        if confirmable {
            // Track for retransmission
            self.pending_requests.insert(
                packet.header.message_id,
                PendingRequest {
                    packet,
                    addr: *addr,
                    sent_at: Instant::now(),
                    retransmits: 0,
                },
            );
        }

        log::debug!("CoAP sent to {}: {} ({} bytes)", addr, path, bytes.len());
        Ok(())
    }

    /// Send message using block-wise transfer
    async fn send_blocks(
        &mut self,
        addr: &SocketAddr,
        path: &str,
        data: &[u8],
        confirmable: bool,
    ) -> Result<()> {
        let total_blocks = data.len().div_ceil(BLOCK_SIZE);

        for block_num in 0..total_blocks {
            let start = block_num * BLOCK_SIZE;
            let end = std::cmp::min(start + BLOCK_SIZE, data.len());
            let block_data = &data[start..end];
            let more = block_num < total_blocks - 1;

            let mut packet = self.create_request_packet(path, block_data, confirmable);

            // Add Block1 option (SZX=2 for 256 bytes)
            // Block1 format: NUM (4+ bits) | M (1 bit) | SZX (3 bits)
            let block1_value = ((block_num as u32) << 4) | (if more { 0x08 } else { 0x00 }) | 0x02;
            let block1_bytes = if block1_value <= 0xFF {
                vec![block1_value as u8]
            } else if block1_value <= 0xFFFF {
                block1_value.to_be_bytes()[2..].to_vec()
            } else {
                block1_value.to_be_bytes().to_vec()
            };
            packet.add_option(CoapOption::Block1, block1_bytes);

            let bytes = packet
                .to_bytes()
                .map_err(|e| Error::Network(format!("Failed to serialize block: {:?}", e)))?;

            {
                let socket = self
                    .socket
                    .as_ref()
                    .ok_or_else(|| Error::Network("Socket not initialized".to_string()))?;
                socket
                    .send_to(&bytes, *addr)
                    .await
                    .map_err(|e| Error::Network(format!("Send block error: {}", e)))?;
            }

            log::trace!("Sent block {}/{} to {}", block_num + 1, total_blocks, addr);
        }

        Ok(())
    }

    /// Create a CoAP request packet
    fn create_request_packet(&mut self, path: &str, payload: &[u8], confirmable: bool) -> Packet {
        let mut packet = Packet::new();

        packet.header.set_version(1);
        packet.header.set_type(if confirmable {
            MessageType::Confirmable
        } else {
            MessageType::NonConfirmable
        });
        packet.header.code = MessageClass::Request(RequestType::Post);
        packet.header.message_id = self.next_message_id();
        packet.set_token(self.generate_token());

        // Add URI path options
        for segment in path.trim_start_matches('/').split('/') {
            if !segment.is_empty() {
                packet.add_option(CoapOption::UriPath, segment.as_bytes().to_vec());
            }
        }

        // Add content format (application/json = 50)
        packet.add_option(CoapOption::ContentFormat, vec![50]);

        packet.payload = payload.to_vec();

        packet
    }

    /// Send a CoAP response
    pub async fn send_response(
        &self,
        addr: &SocketAddr,
        request: &Packet,
        response_code: ResponseType,
        payload: Option<&[u8]>,
    ) -> Result<()> {
        let socket = self
            .socket
            .as_ref()
            .ok_or_else(|| Error::Network("Socket not initialized".to_string()))?;

        let mut response = Packet::new();
        response.header.set_version(1);
        response.header.set_type(MessageType::Acknowledgement);
        response.header.code = MessageClass::Response(response_code);
        response.header.message_id = request.header.message_id;
        response.set_token(request.get_token().to_vec());

        if let Some(data) = payload {
            response.add_option(CoapOption::ContentFormat, vec![50]);
            response.payload = data.to_vec();
        }

        let bytes = response
            .to_bytes()
            .map_err(|e| Error::Network(format!("Failed to serialize response: {:?}", e)))?;

        socket
            .send_to(&bytes, *addr)
            .await
            .map_err(|e| Error::Network(format!("Send response error: {}", e)))?;

        Ok(())
    }

    /// Map message type to CoAP path
    fn message_to_path(message: &Message) -> String {
        match message {
            Message::Ping { .. } => "/ping".to_string(),
            Message::Pong { .. } => "/ping".to_string(),
            Message::GossipRequest { .. } => "/gossip".to_string(),
            Message::GossipResponse { .. } => "/gossip".to_string(),
            Message::NewRecord { .. } => "/announce".to_string(),
            Message::GetRecord { .. } => "/record".to_string(),
            Message::RecordData { .. } => "/record".to_string(),
        }
    }

    /// Get next message ID
    fn next_message_id(&mut self) -> u16 {
        self.message_id = self.message_id.wrapping_add(1);
        self.message_id
    }

    /// Generate a random token
    fn generate_token(&self) -> Vec<u8> {
        let token: [u8; 4] = rand::random();
        token.to_vec()
    }

    /// Handle retransmissions for pending CON messages
    pub async fn handle_retransmissions(&mut self) -> Result<()> {
        let now = Instant::now();
        let timeout = Duration::from_millis(ACK_TIMEOUT_MS);
        let mut to_remove = Vec::new();
        let mut to_retransmit = Vec::new();

        for (msg_id, pending) in &self.pending_requests {
            if now.duration_since(pending.sent_at) > timeout {
                if pending.retransmits >= MAX_RETRANSMIT {
                    to_remove.push(*msg_id);
                    log::warn!(
                        "CoAP message {} timed out after {} retransmits",
                        msg_id,
                        pending.retransmits
                    );
                } else {
                    to_retransmit.push(*msg_id);
                }
            }
        }

        // Remove timed out requests
        for msg_id in to_remove {
            self.pending_requests.remove(&msg_id);
        }

        // Retransmit
        for msg_id in to_retransmit {
            if let Some(pending) = self.pending_requests.get_mut(&msg_id) {
                let bytes = pending
                    .packet
                    .to_bytes()
                    .map_err(|e| Error::Network(format!("Failed to serialize packet: {:?}", e)))?;

                let socket = self
                    .socket
                    .as_ref()
                    .ok_or_else(|| Error::Network("Socket not initialized".to_string()))?;

                if let Err(e) = socket.send_to(&bytes, pending.addr).await {
                    log::warn!("Retransmit failed: {}", e);
                } else {
                    pending.retransmits += 1;
                    pending.sent_at = Instant::now();
                    log::debug!(
                        "Retransmitted CoAP message {} (attempt {})",
                        msg_id,
                        pending.retransmits
                    );
                }
            }
        }

        Ok(())
    }

    /// Handle incoming ACK
    pub fn handle_ack(&mut self, message_id: u16) {
        if self.pending_requests.remove(&message_id).is_some() {
            log::trace!("Received ACK for message {}", message_id);
        }
    }

    /// Join multicast group for discovery
    pub fn join_multicast(&self) -> Result<()> {
        // IPv4 multicast
        if let Some(socket) = &self.socket {
            let socket_ref = socket.get_ref();

            let multicast_addr: std::net::Ipv4Addr = COAP_MULTICAST_IPV4
                .parse()
                .map_err(|e| Error::Network(format!("Invalid multicast address: {}", e)))?;

            socket_ref
                .join_multicast_v4(&multicast_addr, &std::net::Ipv4Addr::UNSPECIFIED)
                .map_err(|e| Error::Network(format!("Failed to join multicast: {}", e)))?;

            log::info!("Joined CoAP multicast group {}", COAP_MULTICAST_IPV4);
        }

        Ok(())
    }

    /// Send multicast discovery request
    pub async fn send_discovery(&mut self) -> Result<()> {
        let multicast_addr: SocketAddr = format!("{}:{}", COAP_MULTICAST_IPV4, COAP_DEFAULT_PORT)
            .parse()
            .map_err(|e| Error::Network(format!("Invalid address: {}", e)))?;

        let mut packet = Packet::new();
        packet.header.set_version(1);
        packet.header.set_type(MessageType::NonConfirmable);
        packet.header.code = MessageClass::Request(RequestType::Get);
        packet.header.message_id = self.next_message_id();
        packet.set_token(self.generate_token());
        packet.add_option(CoapOption::UriPath, b".well-known".to_vec());
        packet.add_option(CoapOption::UriPath, b"core".to_vec());

        if let Some(socket) = &self.socket {
            let bytes = packet
                .to_bytes()
                .map_err(|e| Error::Network(format!("Failed to serialize discovery: {:?}", e)))?;

            socket
                .send_to(&bytes, multicast_addr)
                .await
                .map_err(|e| Error::Network(format!("Discovery send error: {}", e)))?;

            log::debug!("Sent CoAP discovery to {}", multicast_addr);
        }

        Ok(())
    }

    /// Get CoRE Link Format resource description
    pub fn get_core_link_format(&self) -> String {
        "</gossip>;rt=\"aingle.gossip\";ct=50,\
             </record>;rt=\"aingle.record\";ct=50,\
             </announce>;rt=\"aingle.announce\";ct=50,\
             </ping>;rt=\"aingle.ping\"".to_string()
    }
}

/// CoAP configuration
#[derive(Debug, Clone)]
pub struct CoapConfig {
    /// Bind address
    pub bind_addr: String,
    /// Port
    pub port: u16,
    /// Enable multicast discovery
    pub enable_multicast: bool,
    /// Use confirmable messages by default
    pub default_confirmable: bool,
    /// ACK timeout in milliseconds
    pub ack_timeout_ms: u64,
    /// Maximum retransmissions
    pub max_retransmit: u8,
}

impl Default for CoapConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0".to_string(),
            port: COAP_DEFAULT_PORT,
            enable_multicast: true,
            default_confirmable: false, // NON by default for IoT efficiency
            ack_timeout_ms: ACK_TIMEOUT_MS,
            max_retransmit: MAX_RETRANSMIT,
        }
    }
}

impl CoapConfig {
    /// IoT-optimized configuration
    pub fn iot_mode() -> Self {
        Self {
            bind_addr: "0.0.0.0".to_string(),
            port: COAP_DEFAULT_PORT,
            enable_multicast: true,
            default_confirmable: false,
            ack_timeout_ms: 1000, // Faster timeout
            max_retransmit: 2,    // Fewer retries
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coap_config_default() {
        let config = CoapConfig::default();
        assert_eq!(config.port, 5683);
        assert_eq!(config.bind_addr, "0.0.0.0");
        assert!(config.enable_multicast);
        assert!(!config.default_confirmable);
        assert_eq!(config.ack_timeout_ms, ACK_TIMEOUT_MS);
        assert_eq!(config.max_retransmit, MAX_RETRANSMIT);
    }

    #[test]
    fn test_coap_config_iot() {
        let config = CoapConfig::iot_mode();
        assert_eq!(config.ack_timeout_ms, 1000);
        assert_eq!(config.max_retransmit, 2);
        assert!(config.enable_multicast);
        assert!(!config.default_confirmable);
    }

    #[test]
    fn test_coap_config_clone() {
        let config1 = CoapConfig::default();
        let config2 = config1.clone();
        assert_eq!(config1.port, config2.port);
        assert_eq!(config1.bind_addr, config2.bind_addr);
    }

    #[test]
    fn test_coap_config_debug() {
        let config = CoapConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("CoapConfig"));
        assert!(debug_str.contains("5683"));
    }

    #[test]
    fn test_coap_server_creation() {
        let server = CoapServer::new("127.0.0.1".to_string(), 5684, "node1".to_string());
        assert!(!server.is_running());
        assert!(server.socket.is_none());
    }

    #[test]
    fn test_core_link_format() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        let links = server.get_core_link_format();
        assert!(links.contains("/gossip"));
        assert!(links.contains("/record"));
        assert!(links.contains("/announce"));
        assert!(links.contains("/ping"));
        assert!(links.contains("rt="));
        assert!(links.contains("ct=50"));
    }

    #[test]
    fn test_message_to_path_all_variants() {
        use crate::types::Hash;

        // Ping
        assert_eq!(
            CoapServer::message_to_path(&Message::Ping {
                node_id: "test".to_string()
            }),
            "/ping"
        );

        // Pong
        assert_eq!(
            CoapServer::message_to_path(&Message::Pong {
                node_id: "test".to_string(),
                latest_seq: 0
            }),
            "/ping"
        );

        // GossipRequest
        assert_eq!(
            CoapServer::message_to_path(&Message::GossipRequest {
                from_seq: 0,
                limit: 10
            }),
            "/gossip"
        );

        // GossipResponse
        assert_eq!(
            CoapServer::message_to_path(&Message::GossipResponse {
                records: vec![]
            }),
            "/gossip"
        );

        // NewRecord
        let test_hash = Hash::from_bytes(&[0u8; 32]);
        assert_eq!(
            CoapServer::message_to_path(&Message::NewRecord {
                hash: test_hash.clone()
            }),
            "/announce"
        );

        // GetRecord
        assert_eq!(
            CoapServer::message_to_path(&Message::GetRecord {
                hash: test_hash.clone()
            }),
            "/record"
        );

        // Note: RecordData path test skipped as Record construction is complex
        // The path mapping for RecordData is "/record" which is covered above
    }

    #[test]
    fn test_extract_path_empty() {
        let packet = Packet::new();
        let path = CoapServer::extract_path(&packet);
        assert_eq!(path, "/.well-known/core");
    }

    #[test]
    fn test_extract_path_single_segment() {
        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"ping".to_vec());
        let path = CoapServer::extract_path(&packet);
        assert_eq!(path, "/ping");
    }

    #[test]
    fn test_extract_path_multiple_segments() {
        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b".well-known".to_vec());
        packet.add_option(CoapOption::UriPath, b"core".to_vec());
        let path = CoapServer::extract_path(&packet);
        assert_eq!(path, "/.well-known/core");
    }

    #[test]
    fn test_constants() {
        assert_eq!(COAP_MAX_MESSAGE_SIZE, 1024);
        assert_eq!(COAP_DEFAULT_PORT, 5683);
        assert_eq!(COAP_MULTICAST_IPV4, "224.0.1.187");
        assert_eq!(BLOCK_SIZE, 256);
        assert_eq!(MAX_RETRANSMIT, 4);
        assert_eq!(ACK_TIMEOUT_MS, 2000);
    }

    #[test]
    fn test_coap_server_handle_ack() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        // ACK for non-existent message should not panic
        server.handle_ack(12345);

        // Insert a pending request and handle ACK
        let packet = Packet::new();
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        server.pending_requests.insert(
            100,
            PendingRequest {
                packet,
                addr,
                sent_at: Instant::now(),
                retransmits: 0,
            },
        );

        assert!(server.pending_requests.contains_key(&100));
        server.handle_ack(100);
        assert!(!server.pending_requests.contains_key(&100));
    }

    #[test]
    fn test_coap_server_next_message_id() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        let id1 = server.next_message_id();
        let id2 = server.next_message_id();
        assert_eq!(id2, id1.wrapping_add(1));
    }

    #[test]
    fn test_coap_server_generate_token() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        let token1 = server.generate_token();
        let token2 = server.generate_token();

        assert_eq!(token1.len(), 4);
        assert_eq!(token2.len(), 4);
        // Tokens should be different (random)
        // Note: There's a tiny chance they could be equal, but statistically unlikely
    }

    #[test]
    fn test_pending_request_debug() {
        let packet = Packet::new();
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let pending = PendingRequest {
            packet,
            addr,
            sent_at: Instant::now(),
            retransmits: 2,
        };
        let debug_str = format!("{:?}", pending);
        assert!(debug_str.contains("PendingRequest"));
        assert!(debug_str.contains("retransmits: 2"));
    }

    #[test]
    fn test_create_request_packet_non_confirmable() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        let packet = server.create_request_packet("/ping", b"hello", false);

        assert_eq!(packet.header.get_type(), MessageType::NonConfirmable);
        assert_eq!(packet.header.code, MessageClass::Request(RequestType::Post));
        assert_eq!(packet.payload, b"hello");
    }

    #[test]
    fn test_create_request_packet_confirmable() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        let packet = server.create_request_packet("/gossip", b"data", true);

        assert_eq!(packet.header.get_type(), MessageType::Confirmable);
        assert!(!packet.get_token().is_empty());
    }

    #[test]
    fn test_create_request_packet_with_path_segments() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        let packet = server.create_request_packet("/a/b/c", b"", false);

        // Should have multiple UriPath options
        let uri_paths = packet.get_option(CoapOption::UriPath);
        assert!(uri_paths.is_some());
        let paths: Vec<_> = uri_paths.unwrap().iter().collect();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn test_process_packet_ping_with_payload() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"ping".to_vec());
        packet.payload = b"node123".to_vec();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        if let Ok(Some(Message::Ping { node_id })) = result {
            assert_eq!(node_id, "node123");
        }
    }

    #[test]
    fn test_process_packet_ping_empty_payload() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"ping".to_vec());
        // Empty payload

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        if let Ok(Some(Message::Ping { node_id })) = result {
            assert_eq!(node_id, "unknown");
        }
    }

    #[test]
    fn test_process_packet_unknown_path() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"unknown".to_vec());

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_process_packet_gossip_with_valid_json() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"gossip".to_vec());

        let msg = Message::GossipRequest { from_seq: 0, limit: 10 };
        packet.payload = serde_json::to_vec(&msg).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_process_packet_gossip_empty_payload() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"gossip".to_vec());
        // Empty payload

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_process_packet_wellknown_core() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b".well-known".to_vec());
        packet.add_option(CoapOption::UriPath, b"core".to_vec());

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        // Discovery returns None (handled separately)
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_process_packet_record_with_valid_json() {
        use crate::types::Hash;

        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"record".to_vec());

        let test_hash = Hash::from_bytes(&[0u8; 32]);
        let msg = Message::GetRecord { hash: test_hash };
        packet.payload = serde_json::to_vec(&msg).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_process_packet_record_empty_payload() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"record".to_vec());

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_process_packet_announce_with_valid_json() {
        use crate::types::Hash;

        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"announce".to_vec());

        let test_hash = Hash::from_bytes(&[1u8; 32]);
        let msg = Message::NewRecord { hash: test_hash };
        packet.payload = serde_json::to_vec(&msg).unwrap();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_process_packet_announce_empty_payload() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"announce".to_vec());

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_process_packet_invalid_json() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());

        let mut packet = Packet::new();
        packet.add_option(CoapOption::UriPath, b"gossip".to_vec());
        packet.payload = b"invalid json {".to_vec();

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let result = server.process_packet(&packet, &addr);

        assert!(result.is_err());
    }

    #[test]
    fn test_block_state_debug() {
        let state = BlockState {
            data: vec![1, 2, 3],
            block_num: 5,
            more: true,
        };
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("BlockState"));
        assert!(debug_str.contains("block_num: 5"));
        assert!(debug_str.contains("more: true"));
    }

    #[test]
    fn test_block_state_clone() {
        let state1 = BlockState {
            data: vec![1, 2, 3],
            block_num: 10,
            more: false,
        };
        let state2 = state1.clone();
        assert_eq!(state1.data, state2.data);
        assert_eq!(state1.block_num, state2.block_num);
        assert_eq!(state1.more, state2.more);
    }

    #[test]
    fn test_coap_server_message_id_wrapping() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        server.message_id = u16::MAX;
        let id = server.next_message_id();
        assert_eq!(id, 0);
    }

    #[test]
    fn test_message_to_path_record_data() {
        use crate::types::{ActionType, AgentPubKey, EntryType, Record, Signature, Timestamp};
        use crate::types::{Action, Entry};

        let action = Action {
            action_type: ActionType::Create,
            author: AgentPubKey([0u8; 32]),
            timestamp: Timestamp::now(),
            seq: 1,
            prev_action: None,
            entry_hash: None,
            signature: Signature([0u8; 64]),
        };

        let entry = Entry {
            entry_type: EntryType::App,
            content: vec![1, 2, 3],
        };

        let record = Record {
            action,
            entry: Some(entry),
        };

        let msg = Message::RecordData { record };

        assert_eq!(CoapServer::message_to_path(&msg), "/record");
    }

    #[test]
    fn test_join_multicast_no_socket() {
        let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        // Without socket, should return Ok (no-op)
        let result = server.join_multicast();
        assert!(result.is_ok());
    }

    #[test]
    fn test_coap_server_is_running_no_socket() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        assert!(!server.is_running());
        server.running = true;
        // Still false because socket is None
        assert!(!server.is_running());
    }

    #[test]
    fn test_coap_multicast_ipv6_constant() {
        assert_eq!(COAP_MULTICAST_IPV6, "ff02::fd");
    }

    #[test]
    fn test_pending_request_fields() {
        let mut packet = Packet::new();
        packet.payload = vec![1, 2, 3];
        let addr: SocketAddr = "192.168.1.1:5683".parse().unwrap();
        let sent_at = Instant::now();

        let pending = PendingRequest {
            packet,
            addr,
            sent_at,
            retransmits: 3,
        };

        assert_eq!(pending.retransmits, 3);
        assert_eq!(pending.addr.port(), 5683);
        assert_eq!(pending.packet.payload, vec![1, 2, 3]);
    }

    #[test]
    fn test_block_state_fields() {
        let state = BlockState {
            data: vec![10, 20, 30, 40, 50],
            block_num: 42,
            more: true,
        };

        assert_eq!(state.data.len(), 5);
        assert_eq!(state.block_num, 42);
        assert!(state.more);
    }

    #[test]
    fn test_coap_config_custom() {
        let config = CoapConfig {
            bind_addr: "192.168.1.100".to_string(),
            port: 5700,
            enable_multicast: false,
            default_confirmable: true,
            ack_timeout_ms: 5000,
            max_retransmit: 10,
        };

        assert_eq!(config.port, 5700);
        assert!(!config.enable_multicast);
        assert!(config.default_confirmable);
        assert_eq!(config.ack_timeout_ms, 5000);
        assert_eq!(config.max_retransmit, 10);
    }

    #[test]
    fn test_create_request_packet_empty_path() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        let packet = server.create_request_packet("/", b"", false);

        // Should have content format option
        let cf = packet.get_option(CoapOption::ContentFormat);
        assert!(cf.is_some());
    }

    #[test]
    fn test_pending_requests_multiple() {
        let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();

        for i in 0u16..5 {
            let packet = Packet::new();
            server.pending_requests.insert(
                i,
                PendingRequest {
                    packet,
                    addr,
                    sent_at: Instant::now(),
                    retransmits: 0,
                },
            );
        }

        assert_eq!(server.pending_requests.len(), 5);

        // Handle ACKs
        server.handle_ack(0);
        server.handle_ack(2);
        server.handle_ack(4);

        assert_eq!(server.pending_requests.len(), 2);
        assert!(server.pending_requests.contains_key(&1));
        assert!(server.pending_requests.contains_key(&3));
    }

    #[test]
    fn test_coap_server_recv_no_socket() {
        smol::block_on(async {
            let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
            let result = server.recv().await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_coap_server_stop() {
        smol::block_on(async {
            let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
            server.running = true;

            let result = server.stop().await;
            assert!(result.is_ok());
            assert!(!server.is_running());
            assert!(server.socket.is_none());
        });
    }

    #[test]
    fn test_coap_server_send_no_socket() {
        smol::block_on(async {
            let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
            let addr: SocketAddr = "127.0.0.1:5684".parse().unwrap();
            let msg = Message::Ping { node_id: "test".to_string() };

            let result = server.send(&addr, &msg, false).await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_coap_server_send_response_no_socket() {
        smol::block_on(async {
            let server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
            let addr: SocketAddr = "127.0.0.1:5684".parse().unwrap();
            let request = Packet::new();

            let result = server.send_response(&addr, &request, ResponseType::Content, None).await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_coap_server_send_discovery_no_socket() {
        smol::block_on(async {
            let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
            // Without socket, send_discovery should complete (no-op when socket is None)
            let result = server.send_discovery().await;
            // Should return Ok since it just checks if socket exists
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_coap_server_handle_retransmissions_no_socket() {
        smol::block_on(async {
            let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
            // Without pending requests, should complete
            let result = server.handle_retransmissions().await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_coap_server_handle_retransmissions_timed_out() {
        smol::block_on(async {
            let mut server = CoapServer::new("0.0.0.0".to_string(), 5683, "test".to_string());
            let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();

            // Insert a pending request with old timestamp
            let packet = Packet::new();
            server.pending_requests.insert(
                100,
                PendingRequest {
                    packet,
                    addr,
                    sent_at: Instant::now() - Duration::from_secs(10), // Old
                    retransmits: MAX_RETRANSMIT, // Max retries already
                },
            );

            // Should remove timed out request
            let result = server.handle_retransmissions().await;
            assert!(result.is_ok());
            assert!(!server.pending_requests.contains_key(&100));
        });
    }
}
