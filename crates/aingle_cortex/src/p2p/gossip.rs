// Copyright 2019-2026 Apilium Technologies OÜ. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR Commercial

//! Bloom filter gossip adapted for triple synchronization.
//!
//! Ported from `aingle_minimal::gossip` with `Hash` replaced by `[u8; 32]`
//! (compatible with `TripleId.0`).

use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::time::{Duration, Instant};

// ── Constants ────────────────────────────────────────────────────

/// Number of bits in the bloom filter.
const BLOOM_FILTER_BITS: usize = 1024;
/// Number of u64 words in the bloom filter.
const BLOOM_FILTER_WORDS: usize = BLOOM_FILTER_BITS / 64;
/// Number of hash functions.
const BLOOM_HASH_COUNT: usize = 3;
/// Maximum tokens in the rate-limit bucket.
const MAX_BUCKET_TOKENS: f64 = 100.0;
/// Minimum backoff between gossip attempts.
const MIN_BACKOFF: Duration = Duration::from_millis(100);
/// Maximum backoff between gossip attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// Backoff multiplier on failure.
const BACKOFF_MULTIPLIER: f64 = 2.0;

// ── BloomFilter ──────────────────────────────────────────────────

/// Memory-efficient bloom filter operating on `[u8; 32]` keys.
///
/// Uses packed `u64` words (128 bytes for 1024 bits).
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: [u64; BLOOM_FILTER_WORDS],
    hash_count: usize,
    item_count: usize,
    bit_count: usize,
}

impl BloomFilter {
    pub fn new() -> Self {
        Self {
            bits: [0u64; BLOOM_FILTER_WORDS],
            hash_count: BLOOM_HASH_COUNT,
            item_count: 0,
            bit_count: BLOOM_FILTER_BITS,
        }
    }

    pub fn with_capacity(bits: usize, hash_count: usize) -> Self {
        let bit_count = bits.min(BLOOM_FILTER_BITS);
        Self {
            bits: [0u64; BLOOM_FILTER_WORDS],
            hash_count,
            item_count: 0,
            bit_count,
        }
    }

    /// Insert a 32-byte key.
    #[inline]
    pub fn insert(&mut self, key: &[u8; 32]) {
        for i in 0..self.hash_count {
            let index = self.hash_index(key, i);
            let word_idx = index / 64;
            let bit_idx = index % 64;
            self.bits[word_idx] |= 1u64 << bit_idx;
        }
        self.item_count += 1;
    }

    /// Check membership (may have false positives).
    #[inline]
    pub fn may_contain(&self, key: &[u8; 32]) -> bool {
        for i in 0..self.hash_count {
            let index = self.hash_index(key, i);
            let word_idx = index / 64;
            let bit_idx = index % 64;
            if (self.bits[word_idx] & (1u64 << bit_idx)) == 0 {
                return false;
            }
        }
        true
    }

    pub fn clear(&mut self) {
        self.bits = [0u64; BLOOM_FILTER_WORDS];
        self.item_count = 0;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.item_count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.item_count == 0
    }

    pub fn estimated_false_positive_rate(&self) -> f64 {
        if self.item_count == 0 {
            return 0.0;
        }
        let m = self.bit_count as f64;
        let k = self.hash_count as f64;
        let n = self.item_count as f64;
        (1.0 - (-k * n / m).exp()).powf(k)
    }

    #[inline]
    fn hash_index(&self, key: &[u8; 32], seed: usize) -> usize {
        let base = u64::from_le_bytes([
            key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7],
        ]);
        let mixed = base
            .wrapping_mul(0x9e3779b97f4a7c15u64.wrapping_add(seed as u64))
            .wrapping_add(seed as u64);
        let mixed = mixed ^ (mixed >> 33);
        let mixed = mixed.wrapping_mul(0xff51afd7ed558ccdu64);
        (mixed as usize) % self.bit_count
    }

    /// Serialize to 128 bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(BLOOM_FILTER_WORDS * 8);
        for word in &self.bits {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut bits = [0u64; BLOOM_FILTER_WORDS];
        for (i, chunk) in bytes.chunks(8).take(BLOOM_FILTER_WORDS).enumerate() {
            let mut arr = [0u8; 8];
            arr[..chunk.len()].copy_from_slice(chunk);
            bits[i] = u64::from_le_bytes(arr);
        }
        Self {
            bits,
            hash_count: BLOOM_HASH_COUNT,
            item_count: 0,
            bit_count: BLOOM_FILTER_BITS,
        }
    }
}

impl Default for BloomFilter {
    fn default() -> Self {
        Self::new()
    }
}

// ── TokenBucket ──────────────────────────────────────────────────

/// Token-bucket rate limiter for gossip traffic.
#[derive(Debug)]
pub struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a bucket sized for `rate_mbps` megabits/sec.
    pub fn new(rate_mbps: f64) -> Self {
        let refill_rate = (rate_mbps * 125_000.0) / 1024.0;
        Self {
            tokens: MAX_BUCKET_TOKENS,
            max_tokens: MAX_BUCKET_TOKENS,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    pub fn with_params(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    pub fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    pub fn has_tokens(&mut self, tokens: f64) -> bool {
        self.refill();
        self.tokens >= tokens
    }

    pub fn available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
            self.last_refill = now;
        }
    }

    pub fn time_until_available(&mut self, tokens: f64) -> Duration {
        self.refill();
        if self.tokens >= tokens {
            Duration::ZERO
        } else {
            let needed = tokens - self.tokens;
            Duration::from_secs_f64(needed / self.refill_rate)
        }
    }
}

// ── MessagePriority & Queue ──────────────────────────────────────

/// Priority levels for gossip messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

#[derive(Debug)]
pub struct PrioritizedMessage<T> {
    pub message: T,
    pub priority: MessagePriority,
    pub queued_at: Instant,
    sequence: u64,
}

impl<T> PartialEq for PrioritizedMessage<T> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}
impl<T> Eq for PrioritizedMessage<T> {}

impl<T> PartialOrd for PrioritizedMessage<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for PrioritizedMessage<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.priority.cmp(&other.priority) {
            std::cmp::Ordering::Equal => other.sequence.cmp(&self.sequence),
            ord => ord,
        }
    }
}

/// Priority queue for gossip messages.
#[derive(Debug)]
pub struct MessageQueue<T> {
    heap: BinaryHeap<PrioritizedMessage<T>>,
    sequence: u64,
    max_size: usize,
}

impl<T> MessageQueue<T> {
    pub fn new(max_size: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(max_size),
            sequence: 0,
            max_size,
        }
    }

    pub fn push(&mut self, message: T, priority: MessagePriority) -> bool {
        if self.heap.len() >= self.max_size {
            return false;
        }
        self.sequence += 1;
        self.heap.push(PrioritizedMessage {
            message,
            priority,
            queued_at: Instant::now(),
            sequence: self.sequence,
        });
        true
    }

    pub fn pop(&mut self) -> Option<T> {
        self.heap.pop().map(|pm| pm.message)
    }

    pub fn peek(&self) -> Option<&T> {
        self.heap.peek().map(|pm| &pm.message)
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn clear(&mut self) {
        self.heap.clear();
    }
}

impl<T> Default for MessageQueue<T> {
    fn default() -> Self {
        Self::new(100)
    }
}

// ── PeerGossipState ──────────────────────────────────────────────

/// Per-peer gossip state with adaptive backoff.
#[derive(Debug, Clone)]
pub struct PeerGossipState {
    pub failures: u32,
    pub successes: u32,
    pub current_backoff: Duration,
    pub last_attempt: Instant,
    pub known_ids: BloomFilter,
}

impl PeerGossipState {
    pub fn new() -> Self {
        Self {
            failures: 0,
            successes: 0,
            current_backoff: MIN_BACKOFF,
            last_attempt: Instant::now(),
            known_ids: BloomFilter::new(),
        }
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.successes = self.successes.saturating_add(1);
        self.current_backoff = Duration::from_millis(
            (self.current_backoff.as_millis() as f64 / BACKOFF_MULTIPLIER) as u64,
        )
        .max(MIN_BACKOFF);
        self.last_attempt = Instant::now();
    }

    pub fn record_failure(&mut self) {
        self.successes = 0;
        self.failures = self.failures.saturating_add(1);
        self.current_backoff = Duration::from_millis(
            (self.current_backoff.as_millis() as f64 * BACKOFF_MULTIPLIER) as u64,
        )
        .min(MAX_BACKOFF);
        self.last_attempt = Instant::now();
    }

    pub fn should_gossip(&self) -> bool {
        self.last_attempt.elapsed() >= self.current_backoff
    }
}

impl Default for PeerGossipState {
    fn default() -> Self {
        Self::new()
    }
}

// ── TripleGossipManager ──────────────────────────────────────────

/// Gossip manager adapted for triple IDs (`[u8; 32]`).
#[derive(Debug)]
pub struct TripleGossipManager {
    /// Pending IDs to announce.
    pending_announcements: VecDeque<[u8; 32]>,
    /// Local bloom filter of known IDs.
    local_filter: BloomFilter,
    /// Recent IDs (dedup set).
    recent_ids: HashSet<[u8; 32]>,
    max_recent: usize,
    /// Gossip round counter.
    round: u64,
}

impl TripleGossipManager {
    pub fn new() -> Self {
        Self {
            pending_announcements: VecDeque::with_capacity(100),
            local_filter: BloomFilter::new(),
            recent_ids: HashSet::with_capacity(1000),
            max_recent: 1000,
            round: 0,
        }
    }

    /// Register a new local triple for announcement.
    pub fn announce(&mut self, id: [u8; 32]) {
        self.local_filter.insert(&id);

        if self.recent_ids.len() >= self.max_recent {
            self.recent_ids.clear();
        }
        self.recent_ids.insert(id);
        self.pending_announcements.push_back(id);
    }

    /// Check if we already know about an ID.
    pub fn is_known(&self, id: &[u8; 32]) -> bool {
        self.recent_ids.contains(id) || self.local_filter.may_contain(id)
    }

    /// Remove an ID from the recent set (cannot remove from bloom filter).
    pub fn remove_known(&mut self, id: &[u8; 32]) {
        self.recent_ids.remove(id);
    }

    /// Register a known ID (e.g. received from peer).
    pub fn add_known(&mut self, id: [u8; 32]) {
        self.local_filter.insert(&id);
        if self.recent_ids.len() < self.max_recent {
            self.recent_ids.insert(id);
        }
    }

    /// Find IDs that exist in `our_ids` but are missing from `peer_filter`.
    pub fn find_missing(
        &self,
        peer_filter: &BloomFilter,
        our_ids: &[[u8; 32]],
    ) -> Vec<[u8; 32]> {
        our_ids
            .iter()
            .filter(|id| !peer_filter.may_contain(id))
            .copied()
            .collect()
    }

    /// Get a reference to the local bloom filter.
    pub fn get_bloom_filter(&self) -> &BloomFilter {
        &self.local_filter
    }

    /// Drain pending announcements (up to `limit`).
    pub fn take_announcements(&mut self, limit: usize) -> Vec<[u8; 32]> {
        let count = limit.min(self.pending_announcements.len());
        self.pending_announcements.drain(..count).collect()
    }

    pub fn gossip_complete(&mut self) {
        self.round += 1;
    }

    pub fn round(&self) -> u64 {
        self.round
    }

    /// Current statistics.
    pub fn stats(&self) -> GossipStats {
        GossipStats {
            round: self.round,
            pending_announcements: self.pending_announcements.len(),
            known_ids: self.recent_ids.len(),
            bloom_filter_items: self.local_filter.len(),
            bloom_filter_fpr: self.local_filter.estimated_false_positive_rate(),
        }
    }
}

impl Default for TripleGossipManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Gossip statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GossipStats {
    pub round: u64,
    pub pending_announcements: usize,
    pub known_ids: usize,
    pub bloom_filter_items: usize,
    pub bloom_filter_fpr: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── BloomFilter tests ────────────────────────────────────

    #[test]
    fn bloom_insert_contains() {
        let mut filter = BloomFilter::new();
        let key = [1u8; 32];
        assert!(!filter.may_contain(&key));
        filter.insert(&key);
        assert!(filter.may_contain(&key));
        assert_eq!(filter.len(), 1);
    }

    #[test]
    fn bloom_serialization() {
        let mut filter = BloomFilter::new();
        let key = [1u8; 32];
        filter.insert(&key);
        let bytes = filter.to_bytes();
        let restored = BloomFilter::from_bytes(&bytes);
        assert!(restored.may_contain(&key));
    }

    #[test]
    fn bloom_false_positive_rate() {
        let mut filter = BloomFilter::new();
        assert_eq!(filter.estimated_false_positive_rate(), 0.0);
        for i in 0..10u8 {
            filter.insert(&[i; 32]);
        }
        let fpr = filter.estimated_false_positive_rate();
        assert!(fpr > 0.0 && fpr < 1.0);
    }

    #[test]
    fn bloom_to_bytes_size() {
        let filter = BloomFilter::new();
        assert_eq!(filter.to_bytes().len(), 128);
    }

    #[test]
    fn bloom_round_trip_many() {
        let mut filter = BloomFilter::new();
        for i in 0..50u8 {
            filter.insert(&[i; 32]);
        }
        let bytes = filter.to_bytes();
        let restored = BloomFilter::from_bytes(&bytes);
        for i in 0..50u8 {
            assert!(restored.may_contain(&[i; 32]));
        }
    }

    // ── TokenBucket tests ────────────────────────────────────

    #[test]
    fn token_bucket_consume() {
        let mut bucket = TokenBucket::new(1.0);
        assert!(bucket.try_consume(1.0));
        // Drain
        for _ in 0..200 {
            bucket.try_consume(1.0);
        }
        assert!(!bucket.try_consume(100.0));
    }

    #[test]
    fn token_bucket_refill() {
        let mut bucket = TokenBucket::with_params(10.0, 1000.0);
        for _ in 0..15 {
            bucket.try_consume(1.0);
        }
        std::thread::sleep(Duration::from_millis(20));
        assert!(bucket.has_tokens(1.0));
    }

    // ── MessageQueue tests ───────────────────────────────────

    #[test]
    fn message_queue_priority_ordering() {
        let mut queue: MessageQueue<String> = MessageQueue::new(10);
        queue.push("low".into(), MessagePriority::Low);
        queue.push("critical".into(), MessagePriority::Critical);
        queue.push("normal".into(), MessagePriority::Normal);
        queue.push("high".into(), MessagePriority::High);

        assert_eq!(queue.pop(), Some("critical".into()));
        assert_eq!(queue.pop(), Some("high".into()));
        assert_eq!(queue.pop(), Some("normal".into()));
        assert_eq!(queue.pop(), Some("low".into()));
    }

    // ── PeerGossipState tests ────────────────────────────────

    #[test]
    fn peer_gossip_state_backoff() {
        let mut state = PeerGossipState::new();
        std::thread::sleep(MIN_BACKOFF + Duration::from_millis(10));
        assert!(state.should_gossip());

        let initial = state.current_backoff;
        state.record_failure();
        assert!(state.current_backoff > initial);
        assert!(!state.should_gossip());

        std::thread::sleep(state.current_backoff + Duration::from_millis(10));
        assert!(state.should_gossip());

        let before = state.current_backoff;
        state.record_success();
        assert!(state.current_backoff < before);
    }

    // ── TripleGossipManager tests ────────────────────────────

    #[test]
    fn announce_adds_to_filter() {
        let mut mgr = TripleGossipManager::new();
        let id = [42u8; 32];
        mgr.announce(id);
        assert!(mgr.is_known(&id));
    }

    #[test]
    fn find_missing_returns_delta() {
        let mgr = TripleGossipManager::new();
        let a = [1u8; 32];
        let b = [2u8; 32];
        let c = [3u8; 32];
        let d = [4u8; 32];

        let mut peer_filter = BloomFilter::new();
        peer_filter.insert(&a);
        peer_filter.insert(&b);

        let missing = mgr.find_missing(&peer_filter, &[a, b, c, d]);
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&c));
        assert!(missing.contains(&d));
    }

    #[test]
    fn remove_known_from_recent() {
        let mut mgr = TripleGossipManager::new();
        let id = [42u8; 32];
        mgr.announce(id);
        assert!(mgr.recent_ids.contains(&id));
        mgr.remove_known(&id);
        assert!(!mgr.recent_ids.contains(&id));
        // Still in bloom filter (cannot remove), so is_known may still return true
    }

    #[test]
    fn stats_reflect_state() {
        let mut mgr = TripleGossipManager::new();
        mgr.announce([1u8; 32]);
        mgr.announce([2u8; 32]);
        let stats = mgr.stats();
        assert_eq!(stats.pending_announcements, 2);
        assert_eq!(stats.bloom_filter_items, 2);
        assert_eq!(stats.known_ids, 2);
        assert_eq!(stats.round, 0);
    }
}
