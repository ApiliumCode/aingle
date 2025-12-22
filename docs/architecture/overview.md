# AIngle Architecture Overview

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              AIngle Network                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐             │
│  │   Full Node     │  │  Minimal Node   │  │   Smart Node    │             │
│  │   (Server)      │  │   (IoT Device)  │  │   (AI Agent)    │             │
│  ├─────────────────┤  ├─────────────────┤  ├─────────────────┤             │
│  │ • Full DAG      │  │ • Pruned DAG    │  │ • MinimalNode   │             │
│  │ • Validation    │  │ • CoAP Transport│  │ • HOPE Agent    │             │
│  │ • Websocket API │  │ • Gossip Sync   │  │ • Policy Engine │             │
│  │ • App Hosting   │  │ • <1MB RAM      │  │ • Learning      │             │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘             │
│           │                    │                    │                       │
│           └────────────────────┼────────────────────┘                       │
│                                │                                            │
│                    ┌───────────▼───────────┐                                │
│                    │    P2P Network        │                                │
│                    │  ┌─────────────────┐  │                                │
│                    │  │ Kitsune Protocol │  │                                │
│                    │  │ (QUIC/WebRTC)   │  │                                │
│                    │  └─────────────────┘  │                                │
│                    │  ┌─────────────────┐  │                                │
│                    │  │ CoAP Protocol   │  │                                │
│                    │  │ (UDP/Multicast) │  │                                │
│                    │  └─────────────────┘  │                                │
│                    └───────────────────────┘                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## DAG Structure

AIngle uses a Directed Acyclic Graph (DAG) instead of a blockchain for data storage.

```
                     Genesis
                        │
            ┌───────────┼───────────┐
            │           │           │
         Agent A     Agent B     Agent C
            │           │           │
         ┌──┴──┐     ┌──┴──┐     ┌──┴──┐
         │     │     │     │     │     │
        A1    A2    B1    B2    C1    C2
         │     │     │     │     │     │
         └──┬──┘     │     │     └──┬──┘
            │        │     │        │
           A3 ───────┴─────┴─────── C3
            │                       │
            └───────────┬───────────┘
                        │
                      Merge
```

### Key Concepts

1. **Entries**: Data units stored in the DAG
   - App entries: Application data
   - Agent entries: Identity/capability data
   - Links: References between entries

2. **Actions**: Operations on entries
   - Create: Add new entry
   - Update: Modify existing entry
   - Delete: Mark entry as deleted

3. **Records**: Entry + Action pairs with signatures

## Crate Hierarchy

```
aingle (main conductor)
├── aingle_types           # Core types and traits
├── aingle_keystore        # Cryptographic key management
├── aingle_state           # State and database management
├── aingle_conductor_api   # Conductor HTTP/WebSocket API
├── aingle_websocket       # WebSocket implementation
├── aingle_p2p             # Peer-to-peer networking (Kitsune)
├── aingle_ai              # AI integration layer
├── aingle_minimal         # Lightweight IoT node
│   ├── coap              # CoAP transport (RFC 7252)
│   ├── gossip            # Optimized gossip protocol
│   └── smart             # SmartNode (with HOPE agents)
├── hope_agents            # HOPE agent framework
└── titans_memory          # Neural memory system
```

## Data Flow

### Write Path

```
Application
    │
    ▼
Conductor API
    │
    ▼
Validation (Sys → App)
    │
    ▼
Local Storage (DAG)
    │
    ▼
Gossip Network
    │
    ▼
Remote Nodes
```

### Read Path

```
Application
    │
    ▼
Conductor API
    │
    ▼
Local Storage
    │
    └──► Cache Hit ──► Return
    │
    ▼
Network Query (if not found)
    │
    ▼
Remote Nodes
    │
    ▼
Validation
    │
    ▼
Return to Application
```

## IoT Node Architecture (aingle_minimal)

```
┌─────────────────────────────────────────────────────────────┐
│                    Minimal Node                              │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐  Memory Budget: 512KB                     │
│  │   Sensors    │  ┌─────────────────────────────────────┐  │
│  │  ────────────┤  │ Runtime:    128KB                   │  │
│  │ • Temperature│  │ Crypto:      64KB                   │  │
│  │ • Humidity   │  │ Network:    128KB                   │  │
│  │ • Motion     │  │ Storage:    128KB                   │  │
│  │ • GPS        │  │ App:         64KB                   │  │
│  └──────┬───────┘  └─────────────────────────────────────┘  │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐     ┌──────────────┐                      │
│  │  Observation │────▶│  SmartNode   │                      │
│  │   Buffer     │     │  (Optional)  │                      │
│  └──────────────┘     └──────┬───────┘                      │
│                              │                               │
│         ┌────────────────────┼────────────────────┐         │
│         │                    │                    │         │
│         ▼                    ▼                    ▼         │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐ │
│  │   Storage    │     │   Network    │     │   Actions    │ │
│  │   (SQLite)   │     │   (CoAP)     │     │  (Actuators) │ │
│  └──────────────┘     └──────────────┘     └──────────────┘ │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Gossip Protocol

### Standard Gossip Flow

```
Node A                    Node B
  │                         │
  │──── Ping ──────────────▶│
  │                         │
  │◀─── Pong + Seq ─────────│
  │                         │
  │──── GossipRequest ─────▶│
  │     (from_seq, limit)   │
  │                         │
  │◀─── GossipResponse ─────│
  │     (records)           │
  │                         │
```

### Optimized Gossip (with Bloom Filters)

```
Node A                    Node B
  │                         │
  │──── BloomFilter ───────▶│
  │     (known hashes)      │
  │                         │
  │◀─── Missing Hashes ─────│
  │                         │
  │──── Records ───────────▶│
  │                         │
```

## Security Model

1. **Cryptographic Signing**
   - Ed25519 signatures for all actions
   - BLAKE3 hashing for content addressing

2. **Capability-Based Access**
   - Grants for delegating permissions
   - Revocation through the DAG

3. **Validation Rules**
   - System validation (structural integrity)
   - Application validation (business logic)

## Consensus

AIngle uses **eventual consistency** with **adaptive thresholds**:

1. **AI-Enhanced Thresholds**: The AI layer can adjust consensus thresholds based on network conditions
2. **Validation Receipts**: Peers send receipts when they validate data
3. **Confidence Scoring**: Entries gain confidence as more validations accumulate

```
Entry Created ──▶ Local Validation ──▶ Gossip ──▶ Remote Validations
                                                          │
                                                          ▼
                                              Confidence Threshold
                                                          │
                                                          ▼
                                                    Considered Valid
```
