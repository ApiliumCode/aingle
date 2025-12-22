//! Benchmarks for AIngle Minimal Node
//!
//! Run with: cargo bench -p aingle_minimal

use aingle_minimal::crypto::{verify, Keypair};
use aingle_minimal::{Config, Hash, MinimalNode};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Benchmark node creation
fn bench_node_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Node Creation");

    group.bench_function("default_config", |b| {
        b.iter(|| {
            let mut config = Config::default();
            config.storage.db_path = ":memory:".to_string();
            black_box(MinimalNode::new(config))
        });
    });

    group.bench_function("iot_mode", |b| {
        b.iter(|| {
            let mut config = Config::iot_mode();
            config.storage.db_path = ":memory:".to_string();
            black_box(MinimalNode::new(config))
        });
    });

    group.bench_function("low_power_mode", |b| {
        b.iter(|| {
            let mut config = Config::low_power();
            config.storage.db_path = ":memory:".to_string();
            black_box(MinimalNode::new(config))
        });
    });

    group.finish();
}

/// Benchmark crypto operations
fn bench_crypto(c: &mut Criterion) {
    let mut group = c.benchmark_group("Crypto");

    group.bench_function("keypair_generation", |b| {
        b.iter(|| black_box(Keypair::generate()));
    });

    group.bench_function("hash_small", |b| {
        let data = b"small data for hashing";
        b.iter(|| black_box(Hash::from_bytes(data)));
    });

    group.bench_function("hash_1kb", |b| {
        let data = vec![0u8; 1024];
        b.iter(|| black_box(Hash::from_bytes(&data)));
    });

    group.bench_function("hash_10kb", |b| {
        let data = vec![0u8; 10 * 1024];
        b.iter(|| black_box(Hash::from_bytes(&data)));
    });

    group.bench_function("sign_verify", |b| {
        let keypair = Keypair::generate();
        let message = b"test message to sign";

        b.iter(|| {
            let signature = keypair.sign(message);
            black_box(verify(&keypair.public_key(), message, &signature))
        });
    });

    group.finish();
}

/// Benchmark entry creation
fn bench_entry_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Entry Creation");

    group.bench_function("sensor_entry", |b| {
        b.iter(|| {
            let mut config = Config::iot_mode();
            config.storage.db_path = ":memory:".to_string();
            let mut node = MinimalNode::new(config).unwrap();

            let content = serde_json::json!({
                "sensor_id": "temp_001",
                "value": 23.5,
                "unit": "celsius",
                "timestamp": 1234567890
            });

            black_box(node.create_entry(content))
        });
    });

    // Individual creates (baseline)
    for size in [100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("individual_entries", size),
            size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let mut config = Config::iot_mode();
                        config.storage.db_path = ":memory:".to_string();
                        MinimalNode::new(config).unwrap()
                    },
                    |mut node| {
                        for i in 0..size {
                            let content = serde_json::json!({
                                "id": i,
                                "value": i as f64 * 0.1
                            });
                            let _ = node.create_entry(content);
                        }
                        black_box(node.stats())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    // Batch creates (optimized)
    for size in [100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("batch_entries_optimized", size),
            size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let mut config = Config::iot_mode();
                        config.storage.db_path = ":memory:".to_string();
                        let contents: Vec<_> = (0..size)
                            .map(|i| {
                                serde_json::json!({
                                    "id": i,
                                    "value": i as f64 * 0.1
                                })
                            })
                            .collect();
                        (MinimalNode::new(config).unwrap(), contents)
                    },
                    |(mut node, contents)| {
                        let _ = node.create_entries_batch(&contents);
                        black_box(node.stats())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark storage operations
fn bench_storage(c: &mut Criterion) {
    let mut group = c.benchmark_group("Storage");

    group.bench_function("store_and_retrieve", |b| {
        b.iter_batched(
            || {
                let mut config = Config::iot_mode();
                config.storage.db_path = ":memory:".to_string();
                let mut node = MinimalNode::new(config).unwrap();

                let content = serde_json::json!({"test": "data"});
                let hash = node.create_entry(content).unwrap();
                (node, hash)
            },
            |(node, hash)| black_box(node.get_entry(&hash)),
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("node_stats", |b| {
        let mut config = Config::iot_mode();
        config.storage.db_path = ":memory:".to_string();
        let mut node = MinimalNode::new(config).unwrap();

        // Add some entries
        for i in 0..100 {
            let _ = node.create_entry(serde_json::json!({"i": i}));
        }

        b.iter(|| black_box(node.stats()));
    });

    group.finish();
}

/// Benchmark IoT-specific patterns
fn bench_iot_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("IoT Patterns");

    group.bench_function("sensor_reading_cycle", |b| {
        let mut config = Config::iot_mode();
        config.storage.db_path = ":memory:".to_string();
        let mut node = MinimalNode::new(config).unwrap();
        let mut counter = 0u64;

        b.iter(|| {
            counter += 1;

            // Typical IoT cycle: read sensor, store, report
            let reading = serde_json::json!({
                "sensor": "temperature",
                "value": 20.0 + (counter % 100) as f64 * 0.1,
                "seq": counter
            });

            let hash = node.create_entry(reading);
            black_box(hash)
        });
    });

    group.bench_function("burst_upload_10", |b| {
        b.iter_batched(
            || {
                let mut config = Config::iot_mode();
                config.storage.db_path = ":memory:".to_string();
                MinimalNode::new(config).unwrap()
            },
            |mut node| {
                // Simulate buffered sensor readings upload
                let readings: Vec<_> = (0..10)
                    .map(|i| {
                        serde_json::json!({
                            "reading": i,
                            "value": i as f64 * 0.5
                        })
                    })
                    .collect();

                for reading in readings {
                    let _ = node.create_entry(reading);
                }
                black_box(node.stats())
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Benchmark memory footprint estimation
fn bench_memory_footprint(c: &mut Criterion) {
    let mut group = c.benchmark_group("Memory Footprint");

    group.bench_function("minimal_node_size", |b| {
        b.iter(|| {
            let mut config = Config::iot_mode();
            config.storage.db_path = ":memory:".to_string();
            let node = MinimalNode::new(config).unwrap();

            // Estimate struct size
            let size = std::mem::size_of_val(&node);
            black_box(size)
        });
    });

    group.bench_function("config_size", |b| {
        b.iter(|| {
            let config = Config::iot_mode();
            let size = std::mem::size_of_val(&config);
            black_box(size)
        });
    });

    group.bench_function("keypair_size", |b| {
        let keypair = Keypair::generate();
        b.iter(|| {
            let size = std::mem::size_of_val(&keypair);
            black_box(size)
        });
    });

    group.finish();
}

/// Benchmark Gossip Protocol components
fn bench_gossip(c: &mut Criterion) {
    use aingle_minimal::gossip::{BloomFilter, TokenBucket};

    let mut group = c.benchmark_group("Gossip Protocol");

    // BloomFilter benchmarks
    group.bench_function("bloom_filter_create", |b| {
        b.iter(|| black_box(BloomFilter::new()));
    });

    group.bench_function("bloom_filter_insert_100", |b| {
        b.iter(|| {
            let mut filter = BloomFilter::new();
            for i in 0..100 {
                let data = format!("hash_{}", i);
                let hash = Hash::from_bytes(data.as_bytes());
                filter.insert(&hash);
            }
            black_box(filter)
        });
    });

    group.bench_function("bloom_filter_lookup", |b| {
        let mut filter = BloomFilter::new();
        let hashes: Vec<_> = (0..100)
            .map(|i| Hash::from_bytes(format!("hash_{}", i).as_bytes()))
            .collect();
        for hash in &hashes {
            filter.insert(hash);
        }

        b.iter(|| {
            let mut found = 0;
            for hash in &hashes {
                if filter.may_contain(hash) {
                    found += 1;
                }
            }
            black_box(found)
        });
    });

    group.bench_function("bloom_filter_serialize", |b| {
        let mut filter = BloomFilter::new();
        for i in 0..100 {
            let hash = Hash::from_bytes(format!("hash_{}", i).as_bytes());
            filter.insert(&hash);
        }

        b.iter(|| {
            let bytes = filter.to_bytes();
            black_box(bytes)
        });
    });

    group.bench_function("bloom_filter_deserialize", |b| {
        let mut filter = BloomFilter::new();
        for i in 0..100 {
            let hash = Hash::from_bytes(format!("hash_{}", i).as_bytes());
            filter.insert(&hash);
        }
        let bytes = filter.to_bytes();

        b.iter(|| {
            let restored = BloomFilter::from_bytes(&bytes);
            black_box(restored)
        });
    });

    // TokenBucket benchmarks
    group.bench_function("token_bucket_create", |b| {
        b.iter(|| black_box(TokenBucket::new(1.0))); // 1 Mbps
    });

    group.bench_function("token_bucket_consume", |b| {
        let mut bucket = TokenBucket::with_params(1000.0, 100.0);

        b.iter(|| {
            let result = bucket.try_consume(1.0);
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark CoAP Protocol components
fn bench_coap(c: &mut Criterion) {
    use coap_lite::{MessageClass, MessageType, Packet, RequestType};

    let mut group = c.benchmark_group("CoAP Protocol");

    // Packet creation
    group.bench_function("coap_packet_create_get", |b| {
        b.iter(|| {
            let mut packet = Packet::new();
            packet.header.set_type(MessageType::Confirmable);
            packet.header.code = MessageClass::Request(RequestType::Get);
            packet.header.message_id = 12345;
            packet.set_token(vec![0x01, 0x02, 0x03, 0x04]);
            packet.add_option(coap_lite::CoapOption::UriPath, b"temperature".to_vec());
            black_box(packet)
        });
    });

    group.bench_function("coap_packet_create_post", |b| {
        let payload = serde_json::json!({
            "sensor_id": "temp_001",
            "value": 23.5,
            "unit": "celsius"
        });
        let payload_bytes = serde_json::to_vec(&payload).unwrap();

        b.iter(|| {
            let mut packet = Packet::new();
            packet.header.set_type(MessageType::Confirmable);
            packet.header.code = MessageClass::Request(RequestType::Post);
            packet.header.message_id = 12345;
            packet.set_token(vec![0x01, 0x02, 0x03, 0x04]);
            packet.add_option(coap_lite::CoapOption::UriPath, b"sensor".to_vec());
            packet.payload = payload_bytes.clone();
            black_box(packet)
        });
    });

    // Packet serialization
    group.bench_function("coap_packet_serialize", |b| {
        let mut packet = Packet::new();
        packet.header.set_type(MessageType::Confirmable);
        packet.header.code = MessageClass::Request(RequestType::Get);
        packet.header.message_id = 12345;
        packet.set_token(vec![0x01, 0x02, 0x03, 0x04]);
        packet.add_option(coap_lite::CoapOption::UriPath, b"temperature".to_vec());

        b.iter(|| {
            let bytes = packet.to_bytes();
            black_box(bytes)
        });
    });

    group.bench_function("coap_packet_deserialize", |b| {
        let mut packet = Packet::new();
        packet.header.set_type(MessageType::Confirmable);
        packet.header.code = MessageClass::Request(RequestType::Get);
        packet.header.message_id = 12345;
        packet.set_token(vec![0x01, 0x02, 0x03, 0x04]);
        packet.add_option(coap_lite::CoapOption::UriPath, b"temperature".to_vec());
        let bytes = packet.to_bytes().unwrap();

        b.iter(|| {
            let parsed = Packet::from_bytes(&bytes);
            black_box(parsed)
        });
    });

    // Large payload
    group.bench_function("coap_packet_large_payload_1kb", |b| {
        let payload = vec![0u8; 1024];

        b.iter(|| {
            let mut packet = Packet::new();
            packet.header.set_type(MessageType::Confirmable);
            packet.header.code = MessageClass::Request(RequestType::Post);
            packet.header.message_id = 12345;
            packet.payload = payload.clone();
            let bytes = packet.to_bytes();
            black_box(bytes)
        });
    });

    group.finish();
}

/// Benchmark serialization (for WASM and network transfer)
fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("Serialization");

    // JSON serialization (used in entries)
    group.bench_function("json_serialize_small", |b| {
        let data = serde_json::json!({
            "sensor_id": "temp_001",
            "value": 23.5,
            "unit": "celsius",
            "timestamp": 1234567890u64
        });

        b.iter(|| {
            let bytes = serde_json::to_vec(&data);
            black_box(bytes)
        });
    });

    group.bench_function("json_deserialize_small", |b| {
        let data = serde_json::json!({
            "sensor_id": "temp_001",
            "value": 23.5,
            "unit": "celsius",
            "timestamp": 1234567890u64
        });
        let bytes = serde_json::to_vec(&data).unwrap();

        b.iter(|| {
            let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            black_box(parsed)
        });
    });

    group.bench_function("json_serialize_array_100", |b| {
        let data: Vec<_> = (0..100)
            .map(|i| {
                serde_json::json!({
                    "id": i,
                    "value": i as f64 * 0.1,
                    "timestamp": 1234567890u64 + i
                })
            })
            .collect();

        b.iter(|| {
            let bytes = serde_json::to_vec(&data);
            black_box(bytes)
        });
    });

    // Binary hash serialization
    group.bench_function("hash_to_bytes", |b| {
        let hash = Hash::from_bytes(b"test data for hashing");

        b.iter(|| {
            let bytes = hash.as_bytes();
            black_box(bytes)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_node_creation,
    bench_crypto,
    bench_entry_creation,
    bench_storage,
    bench_iot_patterns,
    bench_memory_footprint,
    bench_gossip,
    bench_coap,
    bench_serialization,
);

criterion_main!(benches);
