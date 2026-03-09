// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! P2P configuration with CLI flag parsing and validation.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Configuration for the P2P subsystem.
#[derive(Debug, Clone)]
pub struct P2pConfig {
    /// Whether P2P is enabled (`--p2p` flag).
    pub enabled: bool,
    /// QUIC listen port (`--p2p-port`, default 19091).
    pub port: u16,
    /// Network isolation seed (`--p2p-seed`). Nodes with different seeds reject each other.
    pub seed: Option<String>,
    /// Manually specified peer addresses (`--p2p-peer`, repeatable).
    pub manual_peers: Vec<SocketAddr>,
    /// Enable mDNS discovery (`--p2p-mdns`).
    pub mdns: bool,
    /// Gossip interval in milliseconds (default 5000).
    pub gossip_interval_ms: u64,
    /// Maximum triples per sync batch (default 5000).
    pub sync_batch_size: usize,
    /// Maximum connected peers (default 32).
    pub max_peers: usize,
    /// Directory for persistent data (keypair, etc.).
    pub data_dir: PathBuf,
    /// Max triples accepted per peer per minute (default 1000).
    pub max_triples_per_peer_per_min: usize,
    /// Max triples accepted globally per minute (default 10000).
    pub max_triples_global_per_min: usize,
}

impl Default for P2pConfig {
    fn default() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cortex");
        Self {
            enabled: false,
            port: 19091,
            seed: None,
            manual_peers: Vec::new(),
            mdns: false,
            gossip_interval_ms: 5000,
            sync_batch_size: 5000,
            max_peers: 32,
            data_dir,
            max_triples_per_peer_per_min: 1000,
            max_triples_global_per_min: 10000,
        }
    }
}

impl P2pConfig {
    /// Validate configuration values.
    pub fn validate(&self) -> Result<(), String> {
        if self.port < 1024 {
            return Err(format!(
                "p2p port must be >= 1024, got {}",
                self.port
            ));
        }

        if let Some(ref seed) = self.seed {
            if seed.is_empty() {
                return Err("p2p seed must not be empty".to_string());
            }
            if !seed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err("p2p seed must be alphanumeric (plus _ and -)".to_string());
            }
        }

        if self.sync_batch_size < 100 || self.sync_batch_size > 50000 {
            return Err(format!(
                "sync_batch_size must be 100..50000, got {}",
                self.sync_batch_size
            ));
        }

        if self.gossip_interval_ms < 1000 {
            return Err(format!(
                "gossip_interval_ms must be >= 1000, got {}",
                self.gossip_interval_ms
            ));
        }

        Ok(())
    }

    /// Parse P2P flags from CLI arguments.
    ///
    /// Recognises: `--p2p`, `--p2p-port`, `--p2p-seed`, `--p2p-peer`, `--p2p-mdns`.
    pub fn from_args(args: &[String]) -> P2pConfig {
        let mut cfg = P2pConfig::default();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--p2p" => cfg.enabled = true,
                "--p2p-mdns" => cfg.mdns = true,
                "--p2p-port" => {
                    if i + 1 < args.len() {
                        if let Ok(p) = args[i + 1].parse::<u16>() {
                            cfg.port = p;
                        }
                        i += 1;
                    }
                }
                "--p2p-seed" => {
                    if i + 1 < args.len() {
                        cfg.seed = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--p2p-peer" => {
                    if i + 1 < args.len() {
                        if let Ok(addr) = args[i + 1].parse::<SocketAddr>() {
                            cfg.manual_peers.push(addr);
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(P2pConfig::default().validate().is_ok());
    }

    #[test]
    fn rejects_invalid_port() {
        let mut cfg = P2pConfig::default();
        cfg.port = 0;
        assert!(cfg.validate().is_err());

        cfg.port = 80;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn parses_cli_args() {
        let args: Vec<String> = vec![
            "--p2p",
            "--p2p-port",
            "19091",
            "--p2p-seed",
            "abc123",
            "--p2p-peer",
            "1.2.3.4:19091",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let cfg = P2pConfig::from_args(&args);
        assert!(cfg.enabled);
        assert_eq!(cfg.port, 19091);
        assert_eq!(cfg.seed.as_deref(), Some("abc123"));
        assert_eq!(cfg.manual_peers.len(), 1);
        assert_eq!(
            cfg.manual_peers[0],
            "1.2.3.4:19091".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn rejects_empty_seed() {
        let mut cfg = P2pConfig::default();
        cfg.seed = Some(String::new());
        assert!(cfg.validate().is_err());
    }
}
