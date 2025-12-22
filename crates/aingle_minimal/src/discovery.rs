//! Multi-protocol peer discovery for AIngle nodes
//!
//! Supports both mDNS/DNS-SD and CoAP multicast discovery for automatic
//! peer discovery on local networks.
//!
//! # Discovery Protocols
//! - **mDNS**: Service type `_aingle._udp.local.` (feature: mdns)
//! - **CoAP Multicast**: `/.well-known/core` to 224.0.1.187:5683 (feature: coap)

use crate::error::Result;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

#[cfg(feature = "mdns")]
use std::sync::{Arc, RwLock};

#[cfg(feature = "coap")]
use crate::coap::CoapServer;

#[cfg(feature = "mdns")]
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

/// AIngle mDNS service type
pub const SERVICE_TYPE: &str = "_aingle._udp.local.";

/// Default mDNS port
pub const DEFAULT_PORT: u16 = 5353;

/// Discovered peer information
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    /// Node ID (public key hex)
    pub node_id: String,
    /// IP addresses
    pub addresses: Vec<IpAddr>,
    /// Service port
    pub port: u16,
    /// When discovered
    pub discovered_at: Instant,
    /// Last seen
    pub last_seen: Instant,
    /// TXT record properties
    pub properties: HashMap<String, String>,
}

impl DiscoveredPeer {
    /// Get socket addresses for this peer
    pub fn socket_addrs(&self) -> Vec<SocketAddr> {
        self.addresses
            .iter()
            .map(|ip| SocketAddr::new(*ip, self.port))
            .collect()
    }

    /// Check if peer is still alive (seen within timeout)
    pub fn is_alive(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() < timeout
    }
}

/// mDNS Discovery service for finding AIngle peers
#[cfg(feature = "mdns")]
pub struct Discovery {
    /// Service daemon
    daemon: ServiceDaemon,
    /// Our node ID
    node_id: String,
    /// Our service port
    port: u16,
    /// Discovered peers
    peers: Arc<RwLock<HashMap<String, DiscoveredPeer>>>,
    /// Whether we're registered
    registered: bool,
    /// Shutdown flag
    running: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(feature = "mdns")]
impl Discovery {
    /// Create a new discovery service
    pub fn new(node_id: String, port: u16) -> Result<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| Error::Network(format!("Failed to create mDNS daemon: {}", e)))?;

        Ok(Self {
            daemon,
            node_id,
            port,
            peers: Arc::new(RwLock::new(HashMap::new())),
            registered: false,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Register our service for discovery by others
    pub fn register(&mut self) -> Result<()> {
        if self.registered {
            return Ok(());
        }

        // Get local IP addresses
        let addresses: Vec<IpAddr> = if_addrs::get_if_addrs()
            .map_err(|e| Error::Network(format!("Failed to get local addresses: {}", e)))?
            .into_iter()
            .filter(|iface| !iface.is_loopback())
            .map(|iface| iface.ip())
            .collect();

        if addresses.is_empty() {
            return Err(Error::Network(
                "No network interfaces available".to_string(),
            ));
        }

        // Create service instance name: node_id.service_type
        let instance_name = format!("aingle-{}", &self.node_id[..8.min(self.node_id.len())]);

        // Build TXT record properties
        let mut properties = HashMap::new();
        properties.insert("node_id".to_string(), self.node_id.clone());
        properties.insert("version".to_string(), crate::VERSION.to_string());
        properties.insert("protocol".to_string(), "coap".to_string());

        // Register service
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &format!("{}.local.", instance_name),
            &addresses[0].to_string(),
            self.port,
            properties,
        )
        .map_err(|e| Error::Network(format!("Failed to create service info: {}", e)))?;

        self.daemon
            .register(service_info)
            .map_err(|e| Error::Network(format!("Failed to register mDNS service: {}", e)))?;

        self.registered = true;
        log::info!(
            "Registered mDNS service: {} on port {}",
            instance_name,
            self.port
        );

        Ok(())
    }

    /// Start browsing for peers
    pub fn start_browsing(&mut self) -> Result<()> {
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| Error::Network(format!("Failed to browse mDNS: {}", e)))?;

        let peers = self.peers.clone();
        let running = self.running.clone();
        let our_node_id = self.node_id.clone();

        // Spawn background task to handle discovery events
        std::thread::spawn(move || {
            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                // Try to receive with timeout, ignoring errors (timeout or disconnect)
                if let Ok(event) = receiver.recv_timeout(Duration::from_millis(100)) {
                    Self::handle_event(&peers, &our_node_id, event);
                }
            }
        });

        log::info!("Started mDNS peer discovery");
        Ok(())
    }

    /// Handle a discovery event
    fn handle_event(
        peers: &Arc<RwLock<HashMap<String, DiscoveredPeer>>>,
        our_node_id: &str,
        event: ServiceEvent,
    ) {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                // Extract node_id from TXT record
                let node_id = info
                    .get_properties()
                    .get("node_id")
                    .map(|v| v.val_str().to_string())
                    .unwrap_or_else(|| info.get_fullname().to_string());

                // Skip ourselves
                if node_id == our_node_id {
                    return;
                }

                let addresses: Vec<IpAddr> = info.get_addresses().iter().copied().collect();

                let mut props = HashMap::new();
                for prop in info.get_properties().iter() {
                    props.insert(prop.key().to_string(), prop.val_str().to_string());
                }

                let peer = DiscoveredPeer {
                    node_id: node_id.clone(),
                    addresses,
                    port: info.get_port(),
                    discovered_at: Instant::now(),
                    last_seen: Instant::now(),
                    properties: props,
                };

                log::info!(
                    "Discovered peer: {} at {:?}:{}",
                    node_id,
                    peer.addresses,
                    peer.port
                );

                if let Ok(mut peers) = peers.write() {
                    peers.insert(node_id, peer);
                }
            }
            ServiceEvent::ServiceRemoved(_service_type, fullname) => {
                // Try to find and remove the peer
                if let Ok(mut peers) = peers.write() {
                    // Remove by matching fullname prefix
                    peers.retain(|id, _| !fullname.contains(id));
                }
                log::debug!("Service removed: {}", fullname);
            }
            ServiceEvent::ServiceFound(_service_type, fullname) => {
                log::debug!("Service found: {}", fullname);
            }
            ServiceEvent::SearchStarted(_) => {
                log::debug!("mDNS search started");
            }
            ServiceEvent::SearchStopped(_) => {
                log::debug!("mDNS search stopped");
            }
        }
    }

    /// Get discovered peers
    pub fn get_peers(&self) -> Vec<DiscoveredPeer> {
        self.peers
            .read()
            .map(|peers| peers.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get alive peers (seen within timeout)
    pub fn get_alive_peers(&self, timeout: Duration) -> Vec<DiscoveredPeer> {
        self.get_peers()
            .into_iter()
            .filter(|p| p.is_alive(timeout))
            .collect()
    }

    /// Get socket addresses of all discovered peers
    pub fn get_peer_addrs(&self) -> Vec<SocketAddr> {
        self.get_peers()
            .into_iter()
            .flat_map(|p| p.socket_addrs())
            .collect()
    }

    /// Stop discovery
    pub fn stop(&mut self) -> Result<()> {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);

        if self.registered {
            // Unregister our service
            let instance_name = format!("aingle-{}", &self.node_id[..8.min(self.node_id.len())]);
            let _ = self
                .daemon
                .unregister(&format!("{}.{}", instance_name, SERVICE_TYPE));
            self.registered = false;
        }

        self.daemon.shutdown().ok();
        log::info!("Stopped mDNS discovery");
        Ok(())
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peers.read().map(|p| p.len()).unwrap_or(0)
    }

    /// Discover peers using CoAP multicast (if coap feature enabled)
    #[cfg(feature = "coap")]
    pub async fn discover_coap_multicast(&self, coap_server: &mut CoapServer) -> Result<()> {
        coap_server.send_discovery().await?;
        log::info!("Sent CoAP multicast discovery request");
        Ok(())
    }

    /// Register discovered peer from CoAP discovery response
    pub fn register_coap_peer(&self, node_id: String, addr: SocketAddr) {
        if let Ok(mut peers) = self.peers.write() {
            let peer = DiscoveredPeer {
                node_id: node_id.clone(),
                addresses: vec![addr.ip()],
                port: addr.port(),
                discovered_at: Instant::now(),
                last_seen: Instant::now(),
                properties: {
                    let mut props = HashMap::new();
                    props.insert("protocol".to_string(), "coap".to_string());
                    props
                },
            };
            peers.insert(node_id.clone(), peer);
            log::info!("Registered CoAP peer: {} at {}", node_id, addr);
        }
    }
}

#[cfg(feature = "mdns")]
impl Drop for Discovery {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Stub Discovery for when mdns feature is disabled
#[cfg(not(feature = "mdns"))]
#[allow(dead_code)]
pub struct Discovery {
    node_id: String,
    port: u16,
}

#[cfg(not(feature = "mdns"))]
impl Discovery {
    pub fn new(node_id: String, port: u16) -> Result<Self> {
        Ok(Self { node_id, port })
    }

    pub fn register(&mut self) -> Result<()> {
        log::warn!("mDNS not available (compile with --features mdns)");
        Ok(())
    }

    pub fn start_browsing(&mut self) -> Result<()> {
        log::warn!("mDNS browsing not available (compile with --features mdns)");
        Ok(())
    }

    pub fn get_peers(&self) -> Vec<DiscoveredPeer> {
        Vec::new()
    }

    pub fn get_alive_peers(&self, _timeout: Duration) -> Vec<DiscoveredPeer> {
        Vec::new()
    }

    pub fn get_peer_addrs(&self) -> Vec<SocketAddr> {
        Vec::new()
    }

    pub fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn peer_count(&self) -> usize {
        0
    }

    /// Discover peers using CoAP multicast (stub)
    #[cfg(feature = "coap")]
    pub async fn discover_coap_multicast(&self, _coap_server: &mut CoapServer) -> Result<()> {
        log::warn!("mDNS not available, using CoAP multicast only");
        Ok(())
    }

    /// Register discovered peer from CoAP discovery response (stub)
    pub fn register_coap_peer(&self, _node_id: String, _addr: SocketAddr) {
        // No-op in stub
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovered_peer_socket_addrs() {
        let peer = DiscoveredPeer {
            node_id: "test123".to_string(),
            addresses: vec![
                "192.168.1.100".parse().unwrap(),
                "192.168.1.101".parse().unwrap(),
            ],
            port: 5683,
            discovered_at: Instant::now(),
            last_seen: Instant::now(),
            properties: HashMap::new(),
        };

        let addrs = peer.socket_addrs();
        assert_eq!(addrs.len(), 2);
        assert_eq!(addrs[0].port(), 5683);
    }

    #[test]
    fn test_discovered_peer_is_alive() {
        let peer = DiscoveredPeer {
            node_id: "test123".to_string(),
            addresses: vec!["192.168.1.100".parse().unwrap()],
            port: 5683,
            discovered_at: Instant::now(),
            last_seen: Instant::now(),
            properties: HashMap::new(),
        };

        assert!(peer.is_alive(Duration::from_secs(60)));
        assert!(peer.is_alive(Duration::from_millis(100)));
    }

    #[test]
    #[cfg(not(feature = "mdns"))]
    fn test_discovery_stub() {
        let mut discovery = Discovery::new("test-node".to_string(), 5683).unwrap();
        assert!(discovery.register().is_ok());
        assert!(discovery.start_browsing().is_ok());
        assert_eq!(discovery.get_peers().len(), 0);
        assert!(discovery.stop().is_ok());
    }
}
