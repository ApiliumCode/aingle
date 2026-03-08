//! mDNS-based peer discovery for Cortex P2P.
//!
//! Feature-gated behind `p2p-mdns`. Provides a no-op stub when disabled.

use std::net::SocketAddr;
use std::time::Instant;

/// Discovered peer metadata.
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub node_id: String,
    pub addr: SocketAddr,
    pub seed_hash: String,
    pub last_seen: Instant,
}

// ── Full implementation (p2p-mdns feature) ───────────────────────

#[cfg(feature = "p2p-mdns")]
mod inner {
    use super::DiscoveredPeer;
    use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
    use std::collections::HashMap;
    use std::net::{IpAddr, SocketAddr};
    use std::sync::{Arc, RwLock};
    use std::time::{Duration, Instant};

    const SERVICE_TYPE: &str = "_cortex-sync._udp.local.";

    pub struct P2pDiscovery {
        daemon: ServiceDaemon,
        node_id: String,
        seed_hash: String,
        port: u16,
        peers: Arc<RwLock<HashMap<String, DiscoveredPeer>>>,
        registered: bool,
        running: Arc<std::sync::atomic::AtomicBool>,
    }

    impl P2pDiscovery {
        pub fn new(node_id: String, seed_hash: String, port: u16) -> Result<Self, String> {
            let daemon = ServiceDaemon::new()
                .map_err(|e| format!("mDNS daemon: {}", e))?;
            Ok(Self {
                daemon,
                node_id,
                seed_hash,
                port,
                peers: Arc::new(RwLock::new(HashMap::new())),
                registered: false,
                running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
        }

        pub fn register(&mut self) -> Result<(), String> {
            if self.registered {
                return Ok(());
            }

            let addresses: Vec<IpAddr> = if_addrs::get_if_addrs()
                .map_err(|e| format!("get interfaces: {}", e))?
                .into_iter()
                .filter(|iface| !iface.is_loopback())
                .map(|iface| iface.ip())
                .collect();

            if addresses.is_empty() {
                return Err("no network interfaces".to_string());
            }

            let instance_name = format!(
                "cortex-{}",
                &self.node_id[..8.min(self.node_id.len())]
            );

            let mut props = HashMap::new();
            props.insert("node_id".to_string(), self.node_id.clone());
            props.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
            props.insert("seed_hash".to_string(), self.seed_hash.clone());
            props.insert("p2p_port".to_string(), self.port.to_string());

            let info = ServiceInfo::new(
                SERVICE_TYPE,
                &instance_name,
                &format!("{}.local.", instance_name),
                &addresses[0].to_string(),
                self.port,
                props,
            )
            .map_err(|e| format!("service info: {}", e))?;

            self.daemon
                .register(info)
                .map_err(|e| format!("register: {}", e))?;

            self.registered = true;
            tracing::info!("mDNS registered {} on port {}", instance_name, self.port);
            Ok(())
        }

        pub fn start_browsing(&mut self) -> Result<(), String> {
            self.running
                .store(true, std::sync::atomic::Ordering::SeqCst);

            let receiver = self
                .daemon
                .browse(SERVICE_TYPE)
                .map_err(|e| format!("browse: {}", e))?;

            let peers = self.peers.clone();
            let running = self.running.clone();
            let our_node_id = self.node_id.clone();
            let our_seed_hash = self.seed_hash.clone();

            std::thread::spawn(move || {
                while running.load(std::sync::atomic::Ordering::SeqCst) {
                    if let Ok(event) = receiver.recv_timeout(Duration::from_millis(100)) {
                        if let ServiceEvent::ServiceResolved(info) = event {
                            let node_id = info
                                .get_properties()
                                .get("node_id")
                                .map(|v| v.val_str().to_string())
                                .unwrap_or_default();

                            if node_id == our_node_id || node_id.is_empty() {
                                continue;
                            }

                            let peer_seed_hash = info
                                .get_properties()
                                .get("seed_hash")
                                .map(|v| v.val_str().to_string())
                                .unwrap_or_default();

                            if peer_seed_hash != our_seed_hash {
                                continue;
                            }

                            let p2p_port: u16 = info
                                .get_properties()
                                .get("p2p_port")
                                .and_then(|v| v.val_str().parse().ok())
                                .unwrap_or(info.get_port());

                            if let Some(ip) = info.get_addresses().iter().next() {
                                let addr = SocketAddr::new(ip.to_ip_addr(), p2p_port);
                                let peer = DiscoveredPeer {
                                    node_id: node_id.clone(),
                                    addr,
                                    seed_hash: peer_seed_hash,
                                    last_seen: Instant::now(),
                                };
                                if let Ok(mut map) = peers.write() {
                                    map.insert(node_id, peer);
                                }
                            }
                        }
                    }
                }
            });

            tracing::info!("mDNS browsing started");
            Ok(())
        }

        pub fn get_discovered_peers(&self) -> Vec<DiscoveredPeer> {
            self.peers
                .read()
                .map(|m| m.values().cloned().collect())
                .unwrap_or_default()
        }

        pub fn stop(&mut self) {
            self.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            if self.registered {
                let instance_name = format!(
                    "cortex-{}",
                    &self.node_id[..8.min(self.node_id.len())]
                );
                let _ = self
                    .daemon
                    .unregister(&format!("{}.{}", instance_name, SERVICE_TYPE));
                self.registered = false;
            }
            self.daemon.shutdown().ok();
            tracing::info!("mDNS stopped");
        }
    }

    impl Drop for P2pDiscovery {
        fn drop(&mut self) {
            self.stop();
        }
    }
}

// ── Stub implementation (no mdns feature) ────────────────────────

#[cfg(not(feature = "p2p-mdns"))]
mod inner {
    use super::DiscoveredPeer;

    pub struct P2pDiscovery {
        _node_id: String,
    }

    impl P2pDiscovery {
        pub fn new(node_id: String, _seed_hash: String, _port: u16) -> Result<Self, String> {
            Ok(Self { _node_id: node_id })
        }

        pub fn register(&mut self) -> Result<(), String> {
            Ok(())
        }

        pub fn start_browsing(&mut self) -> Result<(), String> {
            Ok(())
        }

        pub fn get_discovered_peers(&self) -> Vec<DiscoveredPeer> {
            Vec::new()
        }

        pub fn stop(&mut self) {}
    }
}

pub use inner::P2pDiscovery;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_empty_peers() {
        let d = P2pDiscovery::new("nodeid".into(), "seedhash".into(), 19091).unwrap();
        assert!(d.get_discovered_peers().is_empty());
    }

    #[test]
    fn stub_register_is_noop() {
        let mut d = P2pDiscovery::new("nodeid".into(), "seedhash".into(), 19091).unwrap();
        assert!(d.register().is_ok());
    }

    #[test]
    fn discovered_peer_has_required_fields() {
        let peer = DiscoveredPeer {
            node_id: "abc123".into(),
            addr: "127.0.0.1:19091".parse().unwrap(),
            seed_hash: "hash".into(),
            last_seen: Instant::now(),
        };
        assert_eq!(peer.node_id, "abc123");
        assert_eq!(peer.addr.port(), 19091);
    }
}
