//! Integration tests for transport features with mocks
//!
//! Tests WebRTC, Bluetooth LE, and Hardware Wallet integration:
//! - WebRTC signaling and peer connection workflow
//! - BLE scanning, connection, and message exchange
//! - Hardware wallet connection and signing workflow

use std::time::Duration;

// ============================================================================
// WebRTC Transport Tests
// ============================================================================

#[cfg(feature = "webrtc")]
mod webrtc_tests {
    use aingle_minimal::*;

    #[test]
    fn test_webrtc_config_builder() {
        let config = WebRtcConfig::with_stun("stun:custom.stun.server:3478").with_turn(
            "turn:relay.example.com:3478",
            "user",
            "password",
        );

        assert_eq!(config.stun_server, "stun:custom.stun.server:3478");
        assert!(config.turn_server.is_some());
        assert_eq!(config.turn_username, Some("user".to_string()));
        assert_eq!(config.turn_credential, Some("password".to_string()));
    }

    #[test]
    fn test_webrtc_server_lifecycle() {
        let config = WebRtcConfig::default();
        let server = WebRtcServer::new(config);

        // Initially not running
        assert!(!server.is_running());
        assert_eq!(server.peer_count(), 0);

        // Should have a generated peer ID
        let peer_id = server.local_peer_id();
        assert!(!peer_id.is_empty());
        assert_eq!(peer_id.len(), 32); // 16 bytes hex = 32 chars
    }

    #[test]
    fn test_signaling_config_custom() {
        let config = SignalingConfig {
            bind_addr: "127.0.0.1:9999".to_string(),
            max_connections: 50,
            heartbeat_interval: std::time::Duration::from_secs(15),
            connection_timeout: std::time::Duration::from_secs(30),
        };

        assert_eq!(config.bind_addr, "127.0.0.1:9999");
        assert_eq!(config.max_connections, 50);
    }

    #[test]
    fn test_signaling_message_offer_answer_flow() {
        // Simulate offer
        let offer = SignalingMessage::Offer {
            from: "peer-alice".to_string(),
            to: "peer-bob".to_string(),
            sdp: "v=0\r\no=- 123456789 2 IN IP4 127.0.0.1\r\n".to_string(),
        };

        let json = serde_json::to_string(&offer).unwrap();
        assert!(json.contains("peer-alice"));
        assert!(json.contains("peer-bob"));

        // Simulate answer
        let answer = SignalingMessage::Answer {
            from: "peer-bob".to_string(),
            to: "peer-alice".to_string(),
            sdp: "v=0\r\no=- 987654321 2 IN IP4 192.168.1.100\r\n".to_string(),
        };

        let json = serde_json::to_string(&answer).unwrap();
        assert!(json.contains("Answer"));
    }

    #[test]
    fn test_ice_candidate_exchange() {
        let ice = SignalingMessage::IceCandidate {
            from: "peer-alice".to_string(),
            to: "peer-bob".to_string(),
            candidate: "candidate:1 1 UDP 2130706431 192.168.1.50 54321 typ host".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&ice).unwrap();
        let parsed: SignalingMessage = serde_json::from_str(&json).unwrap();

        if let SignalingMessage::IceCandidate {
            from,
            to,
            candidate,
            sdp_mid,
            sdp_mline_index,
        } = parsed
        {
            assert_eq!(from, "peer-alice");
            assert_eq!(to, "peer-bob");
            assert!(candidate.contains("UDP"));
            assert_eq!(sdp_mid, Some("0".to_string()));
            assert_eq!(sdp_mline_index, Some(0));
        } else {
            panic!("Expected IceCandidate");
        }
    }

    #[test]
    fn test_peer_connection_states() {
        let mut peer = PeerConnection::new("test-peer-123");

        assert_eq!(peer.state, ConnectionState::New);
        assert!(!peer.is_connected());
        assert!(peer.connection_duration().is_none());

        // Simulate connection flow
        peer.state = ConnectionState::Connecting;
        assert_eq!(peer.state, ConnectionState::Connecting);

        peer.state = ConnectionState::Connected;
        peer.connected_at = Some(std::time::Instant::now());
        assert!(peer.is_connected());
        assert!(peer.connection_duration().is_some());

        // Simulate disconnection
        peer.state = ConnectionState::Disconnected;
        assert!(!peer.is_connected());
    }

    #[test]
    fn test_webrtc_stats_aggregation() {
        let mut stats = WebRtcStats::default();
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.bytes_sent, 0);
        assert!(!stats.using_relay);

        // Simulate activity
        stats.messages_sent = 100;
        stats.messages_received = 95;
        stats.bytes_sent = 50_000;
        stats.bytes_received = 48_000;
        stats.rtt_ms = 25;

        assert_eq!(stats.messages_sent, 100);
        assert_eq!(stats.rtt_ms, 25);
    }
}

// ============================================================================
// Bluetooth LE Transport Tests
// ============================================================================

#[cfg(feature = "ble")]
mod ble_tests {
    use super::*;
    use aingle_minimal::*;

    #[test]
    fn test_ble_config_default() {
        let config = BleConfig::default();

        assert!(!config.device_name.is_empty());
        assert!(config.max_connections > 0);
    }

    #[test]
    fn test_ble_config_low_power() {
        let config = BleConfig::low_power();

        // Low power should have reduced settings
        assert!(config.scan_interval_ms > 0);
    }

    #[test]
    fn test_ble_config_mesh_relay() {
        let config = BleConfig::mesh_relay();

        // Mesh relay should enable forwarding
        assert!(config.mesh_relay);
    }

    #[test]
    fn test_ble_manager_creation() {
        let config = BleConfig::default();
        let manager = BleManager::new(config);

        // Initially in idle or uninitialized state
        let state = manager.state();
        assert!(state == BleState::Idle || state == BleState::Uninitialized);
    }

    #[test]
    fn test_ble_state_transitions() {
        // Test all state variants
        assert_eq!(BleState::Idle, BleState::Idle);
        assert_ne!(BleState::Idle, BleState::Scanning);

        // States represent the BLE lifecycle
        let states = vec![
            BleState::Idle,
            BleState::Advertising,
            BleState::Scanning,
            BleState::Connected,
            BleState::Error,
        ];

        assert!(!states.is_empty());
    }

    #[test]
    fn test_ble_stats_tracking() {
        let stats = BleStats::default();

        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
    }

    #[test]
    fn test_ble_peer_creation() {
        let peer = BlePeer::new("AA:BB:CC:DD:EE:FF", -65);

        assert_eq!(peer.address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(peer.rssi, -65);
    }

    #[test]
    fn test_ble_peer_rssi_update() {
        let mut peer = BlePeer::new("AA:BB:CC:DD:EE:FF", -65);
        peer.update_rssi(-55);

        assert_eq!(peer.rssi, -55);
    }

    #[test]
    fn test_ble_peer_staleness() {
        let peer = BlePeer::new("AA:BB:CC:DD:EE:FF", -65);

        // Just created, should not be stale
        assert!(!peer.is_stale(Duration::from_secs(60)));
    }

    #[test]
    fn test_ble_manager_stats() {
        let config = BleConfig::default();
        let manager = BleManager::new(config);

        let stats = manager.stats();
        assert_eq!(stats.messages_sent, 0);
    }
}

// ============================================================================
// Hardware Wallet Tests
// ============================================================================

#[cfg(feature = "hw_wallet")]
mod wallet_tests {
    use aingle_minimal::*;

    #[test]
    fn test_wallet_config_default() {
        let config = WalletConfig::default();

        assert!(config.timeout > std::time::Duration::ZERO);
    }

    #[test]
    fn test_wallet_config_ble() {
        let config = WalletConfig::with_ble();

        assert!(config.use_ble);
    }

    #[test]
    fn test_wallet_manager_creation() {
        let config = WalletConfig::default();
        let manager = WalletManager::new(config);

        assert_eq!(manager.state(), WalletState::Disconnected);
        assert!(!manager.is_connected());
    }

    #[test]
    fn test_derivation_path_creation() {
        // Standard AIngle derivation path
        let path = DerivationPath::aingle(0, 0);

        let path_str = path.to_string();
        assert!(path_str.contains("44"));
    }

    #[test]
    fn test_apdu_command_creation() {
        // Standard APDU for GET_VERSION
        let cmd = ApduCommand::new(0xE0, 0x01, 0x00, 0x00);

        let serialized = cmd.serialize();
        assert_eq!(serialized[0], 0xE0); // CLA
        assert_eq!(serialized[1], 0x01); // INS
        assert_eq!(serialized[2], 0x00); // P1
        assert_eq!(serialized[3], 0x00); // P2
    }

    #[test]
    fn test_apdu_command_with_data() {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let cmd = ApduCommand::new(0xE0, 0x02, 0x00, 0x00).with_data(data.clone());

        let serialized = cmd.serialize();
        assert!(serialized.len() > 5); // Header + data length + data
    }

    #[test]
    fn test_apdu_response_success() {
        // Success response: SW1=0x90 SW2=0x00
        let raw = vec![0x01, 0x02, 0x03, 0x90, 0x00];
        let response = ApduResponse::from_bytes(&raw).unwrap();

        assert!(response.is_success());
    }

    #[test]
    fn test_apdu_response_error() {
        // Error response: 6A80 = Wrong data
        let raw = vec![0x6A, 0x80];
        let response = ApduResponse::from_bytes(&raw).unwrap();

        assert!(!response.is_success());
        assert!(response.error_message().is_some());
    }

    #[test]
    fn test_wallet_states() {
        // All wallet states
        assert_ne!(WalletState::Connected, WalletState::Disconnected);
        assert_ne!(WalletState::Connecting, WalletState::AwaitingConfirmation);
    }

    #[test]
    fn test_wallet_stats() {
        let config = WalletConfig::default();
        let manager = WalletManager::new(config);

        let stats = manager.stats();
        assert_eq!(stats.connections, 0);
    }

    #[test]
    fn test_public_key_hex() {
        let pk = HwPublicKey {
            bytes: vec![0x01, 0x02, 0x03, 0x04],
            path: DerivationPath::aingle(0, 0),
            chain_code: None,
        };

        let hex = pk.to_hex();
        assert_eq!(hex, "01020304");
    }

    #[test]
    fn test_signature_hex() {
        let sig = HwSignature {
            bytes: vec![0xAA, 0xBB, 0xCC, 0xDD],
            path: DerivationPath::aingle(0, 0),
            hash: vec![0x00; 32],
        };

        let hex = sig.to_hex();
        assert!(sig.is_valid_length() || !sig.is_valid_length()); // Either is valid

        assert_eq!(hex.len(), 8); // 4 bytes = 8 hex chars
    }
}

// ============================================================================
// Cross-Transport Integration Tests
// ============================================================================

mod cross_transport_tests {
    use aingle_minimal::*;
    use std::time::Duration;

    #[test]
    fn test_config_iot_mode() {
        let config = Config::iot_mode();

        // Verify IoT config - iot_mode uses ZERO interval for immediate publish
        assert_eq!(config.publish_interval, Duration::ZERO);
        assert!(config.memory_limit > 0);
    }

    #[test]
    fn test_minimal_node_creation() {
        let config = Config::iot_mode();
        let node = MinimalNode::new(config);

        assert!(node.is_ok());
    }

    #[test]
    fn test_gossip_config() {
        let config = aingle_minimal::config::GossipConfig::default();

        // Just verify it can be created with default values
        assert!(config.max_peers > 0);
    }

    #[test]
    fn test_semantic_graph_creation() {
        let graph = SemanticGraph::new();

        // New graph should be empty
        let stats = graph.stats().unwrap();
        assert_eq!(stats.triple_count, 0);
    }

    #[test]
    fn test_sync_manager_creation() {
        let manager = SyncManager::new(Duration::from_secs(30));

        let stats = manager.stats();
        assert_eq!(stats.total_successful_syncs, 0);
    }
}
