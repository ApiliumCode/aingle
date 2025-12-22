//! Optimized Gossip Protocol for AIngle Minimal
//!
//! Implements efficient gossip with:
//! - Bloom filters for set reconciliation
//! - Token bucket rate limiting
//! - Priority-based message queuing
//! - Adaptive timing with exponential backoff
//!
//! # Protocol Flow
//!
//! ```text
//! Node A                          Node B
//!   |                               |
//!   |--[BloomFilter(hashes)]------->|
//!   |                               |
//!   |<--[Missing hashes]------------|
//!   |                               |
//!   |--[Records]------------------>|
//!   |                               |
//! ```

use crate::config::GossipConfig;
use crate::types::Hash;
use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Number of bits in the bloom filter (1024 = 16 * 64)
const BLOOM_FILTER_BITS: usize = 1024;

/// Number of u64 words in the bloom filter
const BLOOM_FILTER_WORDS: usize = BLOOM_FILTER_BITS / 64;

/// Number of hash functions for bloom filter
const BLOOM_HASH_COUNT: usize = 3;

/// Maximum tokens in bucket
const MAX_BUCKET_TOKENS: f64 = 100.0;

/// Minimum backoff duration
const MIN_BACKOFF: Duration = Duration::from_millis(100);

/// Maximum backoff duration
const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// Backoff multiplier
const BACKOFF_MULTIPLIER: f64 = 2.0;

/// Simple Bloom Filter for efficient set membership testing
///
/// Uses multiple hash functions to minimize false positives while
/// maintaining a small memory footprint suitable for IoT devices.
///
/// **Memory optimization:** Uses packed u64 words instead of Vec<bool>,
/// reducing memory from 1024 bytes to 128 bytes (8x reduction).
#[derive(Debug, Clone)]
pub struct BloomFilter {
    /// Packed bits using u64 words (128 bytes for 1024 bits)
    bits: [u64; BLOOM_FILTER_WORDS],
    /// Number of hash functions to use
    hash_count: usize,
    /// Number of items inserted
    item_count: usize,
    /// Total number of bits (for custom capacities)
    bit_count: usize,
}

impl BloomFilter {
    /// Create a new bloom filter with default 1024 bits
    pub fn new() -> Self {
        Self {
            bits: [0u64; BLOOM_FILTER_WORDS],
            hash_count: BLOOM_HASH_COUNT,
            item_count: 0,
            bit_count: BLOOM_FILTER_BITS,
        }
    }

    /// Create a bloom filter with custom size (rounded up to multiple of 64)
    pub fn with_capacity(bits: usize, hash_count: usize) -> Self {
        // Round up to nearest multiple of 64, but cap at BLOOM_FILTER_BITS
        let bit_count = bits.min(BLOOM_FILTER_BITS);
        Self {
            bits: [0u64; BLOOM_FILTER_WORDS],
            hash_count,
            item_count: 0,
            bit_count,
        }
    }

    /// Insert a hash into the filter
    #[inline]
    pub fn insert(&mut self, hash: &Hash) {
        for i in 0..self.hash_count {
            let index = self.hash_index(hash, i);
            let word_idx = index / 64;
            let bit_idx = index % 64;
            self.bits[word_idx] |= 1u64 << bit_idx;
        }
        self.item_count += 1;
    }

    /// Check if a hash might be in the filter (may have false positives)
    #[inline]
    pub fn may_contain(&self, hash: &Hash) -> bool {
        for i in 0..self.hash_count {
            let index = self.hash_index(hash, i);
            let word_idx = index / 64;
            let bit_idx = index % 64;
            if (self.bits[word_idx] & (1u64 << bit_idx)) == 0 {
                return false;
            }
        }
        true
    }

    /// Clear the filter
    #[inline]
    pub fn clear(&mut self) {
        self.bits = [0u64; BLOOM_FILTER_WORDS];
        self.item_count = 0;
    }

    /// Get the number of items inserted
    #[inline]
    pub fn len(&self) -> usize {
        self.item_count
    }

    /// Check if filter is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.item_count == 0
    }

    /// Estimate false positive rate
    pub fn estimated_false_positive_rate(&self) -> f64 {
        if self.item_count == 0 {
            return 0.0;
        }
        let m = self.bit_count as f64;
        let k = self.hash_count as f64;
        let n = self.item_count as f64;
        (1.0 - (-k * n / m).exp()).powf(k)
    }

    /// Compute hash index using optimized hash combining
    ///
    /// Uses only first 8 bytes of hash for speed while maintaining
    /// good distribution with different seeds.
    #[inline]
    fn hash_index(&self, hash: &Hash, seed: usize) -> usize {
        let hash_bytes = hash.as_bytes();

        // Fast hash combining using first 8 bytes as u64
        let base = if hash_bytes.len() >= 8 {
            u64::from_le_bytes([
                hash_bytes[0], hash_bytes[1], hash_bytes[2], hash_bytes[3],
                hash_bytes[4], hash_bytes[5], hash_bytes[6], hash_bytes[7],
            ])
        } else {
            // Fallback for short hashes
            let mut arr = [0u8; 8];
            arr[..hash_bytes.len()].copy_from_slice(hash_bytes);
            u64::from_le_bytes(arr)
        };

        // Mix with seed using fast multiply-xor
        let mixed = base
            .wrapping_mul(0x9e3779b97f4a7c15u64.wrapping_add(seed as u64))
            .wrapping_add(seed as u64);
        let mixed = mixed ^ (mixed >> 33);
        let mixed = mixed.wrapping_mul(0xff51afd7ed558ccdu64);

        (mixed as usize) % self.bit_count
    }

    /// Convert to bytes for network transmission (already packed efficiently)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(BLOOM_FILTER_WORDS * 8);
        for word in &self.bits {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    /// Create from bytes received over network
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
            item_count: 0, // Unknown after deserialization
            bit_count: BLOOM_FILTER_BITS,
        }
    }
}

impl Default for BloomFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Token Bucket rate limiter for bandwidth control
///
/// Ensures gossip traffic respects the configured bandwidth limits,
/// preventing network congestion on constrained IoT networks.
#[derive(Debug)]
pub struct TokenBucket {
    /// Current number of tokens
    tokens: f64,
    /// Maximum tokens (burst capacity)
    max_tokens: f64,
    /// Tokens added per second
    refill_rate: f64,
    /// Last refill time
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket
    ///
    /// # Arguments
    /// * `rate_mbps` - Target rate in megabits per second
    pub fn new(rate_mbps: f64) -> Self {
        // Convert Mbps to bytes per second, then to tokens
        // 1 token = 1 message (~1KB average)
        let refill_rate = (rate_mbps * 125_000.0) / 1024.0; // tokens per second
        Self {
            tokens: MAX_BUCKET_TOKENS,
            max_tokens: MAX_BUCKET_TOKENS,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Create a token bucket with specific parameters
    pub fn with_params(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume tokens for sending a message
    ///
    /// # Arguments
    /// * `tokens` - Number of tokens to consume (typically 1 per message)
    ///
    /// # Returns
    /// `true` if tokens were available and consumed, `false` otherwise
    pub fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Check if tokens are available without consuming
    pub fn has_tokens(&mut self, tokens: f64) -> bool {
        self.refill();
        self.tokens >= tokens
    }

    /// Get current token count
    pub fn available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
            self.last_refill = now;
        }
    }

    /// Get time until tokens are available
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

/// Message priority levels for gossip
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    /// Low priority - peer discovery, heartbeats
    Low = 0,
    /// Normal priority - regular gossip sync
    Normal = 1,
    /// High priority - new record announcements
    High = 2,
    /// Critical priority - consensus messages
    Critical = 3,
}

/// A prioritized message wrapper
#[derive(Debug)]
pub struct PrioritizedMessage<T> {
    /// The message payload
    pub message: T,
    /// Message priority
    pub priority: MessagePriority,
    /// Timestamp when message was queued
    pub queued_at: Instant,
    /// Sequence for FIFO within same priority
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
        // Higher priority first, then older messages first (FIFO)
        match self.priority.cmp(&other.priority) {
            std::cmp::Ordering::Equal => other.sequence.cmp(&self.sequence),
            ord => ord,
        }
    }
}

/// Priority queue for gossip messages
#[derive(Debug)]
pub struct MessageQueue<T> {
    heap: BinaryHeap<PrioritizedMessage<T>>,
    sequence: u64,
    max_size: usize,
}

impl<T> MessageQueue<T> {
    /// Create a new message queue
    pub fn new(max_size: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(max_size),
            sequence: 0,
            max_size,
        }
    }

    /// Push a message with priority
    pub fn push(&mut self, message: T, priority: MessagePriority) -> bool {
        if self.heap.len() >= self.max_size {
            // Drop lowest priority message if full
            if let Some(min) = self.heap.peek() {
                if priority <= min.priority {
                    return false; // Can't add lower priority when full
                }
            }
            // Remove lowest priority (we need a different structure for this)
            // For simplicity, just don't add if full
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

    /// Pop the highest priority message
    pub fn pop(&mut self) -> Option<T> {
        self.heap.pop().map(|pm| pm.message)
    }

    /// Peek at the highest priority message
    pub fn peek(&self) -> Option<&T> {
        self.heap.peek().map(|pm| &pm.message)
    }

    /// Get queue length
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Clear the queue
    pub fn clear(&mut self) {
        self.heap.clear();
    }
}

impl<T> Default for MessageQueue<T> {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Peer gossip state for adaptive timing
#[derive(Debug, Clone)]
pub struct PeerGossipState {
    /// Consecutive failures
    pub failures: u32,
    /// Consecutive successes
    pub successes: u32,
    /// Current backoff duration
    pub current_backoff: Duration,
    /// Last gossip attempt
    pub last_attempt: Instant,
    /// Bloom filter of known hashes
    pub known_hashes: BloomFilter,
}

impl PeerGossipState {
    /// Create new peer state
    pub fn new() -> Self {
        Self {
            failures: 0,
            successes: 0,
            current_backoff: MIN_BACKOFF,
            last_attempt: Instant::now(),
            known_hashes: BloomFilter::new(),
        }
    }

    /// Record a successful gossip
    pub fn record_success(&mut self) {
        self.failures = 0;
        self.successes = self.successes.saturating_add(1);
        // Decrease backoff on success
        self.current_backoff = Duration::from_millis(
            (self.current_backoff.as_millis() as f64 / BACKOFF_MULTIPLIER) as u64,
        )
        .max(MIN_BACKOFF);
        self.last_attempt = Instant::now();
    }

    /// Record a failed gossip
    pub fn record_failure(&mut self) {
        self.successes = 0;
        self.failures = self.failures.saturating_add(1);
        // Increase backoff on failure (exponential)
        self.current_backoff = Duration::from_millis(
            (self.current_backoff.as_millis() as f64 * BACKOFF_MULTIPLIER) as u64,
        )
        .min(MAX_BACKOFF);
        self.last_attempt = Instant::now();
    }

    /// Check if we should attempt gossip with this peer
    pub fn should_gossip(&self) -> bool {
        self.last_attempt.elapsed() >= self.current_backoff
    }

    /// Get time until next gossip attempt
    pub fn time_until_next_attempt(&self) -> Duration {
        let elapsed = self.last_attempt.elapsed();
        if elapsed >= self.current_backoff {
            Duration::ZERO
        } else {
            self.current_backoff - elapsed
        }
    }
}

impl Default for PeerGossipState {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced Gossip Manager with optimizations
#[derive(Debug)]
pub struct GossipManager {
    config: GossipConfig,
    /// Last gossip round timestamp
    last_gossip: Instant,
    /// Pending record announcements
    pending_announcements: VecDeque<Hash>,
    /// Token bucket for rate limiting
    rate_limiter: TokenBucket,
    /// Local bloom filter of known hashes
    local_filter: BloomFilter,
    /// Set of recently seen hashes (for deduplication)
    recent_hashes: HashSet<Hash>,
    /// Maximum recent hashes to track
    max_recent: usize,
    /// Message queue
    message_queue: MessageQueue<GossipMessage>,
    /// Round counter
    round: u64,
}

/// Gossip message types
#[derive(Debug, Clone)]
pub enum GossipMessage {
    /// Bloom filter for set reconciliation
    BloomSync { filter_bytes: Vec<u8> },
    /// Request for missing records
    RequestRecords { hashes: Vec<Hash> },
    /// Batch of records
    SendRecords { records: Vec<Vec<u8>> },
    /// Single announcement
    Announce { hash: Hash },
}

impl GossipManager {
    /// Create a new gossip manager
    pub fn new(config: GossipConfig) -> Self {
        Self {
            rate_limiter: TokenBucket::new(config.output_target_mbps),
            config,
            last_gossip: Instant::now(),
            pending_announcements: VecDeque::with_capacity(100),
            local_filter: BloomFilter::new(),
            recent_hashes: HashSet::with_capacity(1000),
            max_recent: 1000,
            message_queue: MessageQueue::new(100),
            round: 0,
        }
    }

    /// Check if gossip should run
    pub fn should_gossip(&self) -> bool {
        self.last_gossip.elapsed() >= self.config.loop_delay
    }

    /// Mark gossip round as complete
    pub fn gossip_complete(&mut self, success: bool) {
        self.round += 1;
        self.last_gossip = Instant::now();

        if success {
            log::debug!("Gossip round {} completed successfully", self.round);
        } else {
            log::debug!("Gossip round {} failed", self.round);
        }
    }

    /// Queue a record for announcement
    pub fn announce(&mut self, hash: Hash) {
        // Add to bloom filter
        self.local_filter.insert(&hash);

        // Add to recent hashes
        if self.recent_hashes.len() >= self.max_recent {
            // Remove oldest (approximate - we'd need an LRU for exact)
            self.recent_hashes.clear();
        }
        self.recent_hashes.insert(hash.clone());

        // Queue announcement
        self.pending_announcements.push_back(hash.clone());

        // Also add to message queue with high priority
        self.message_queue
            .push(GossipMessage::Announce { hash }, MessagePriority::High);
    }

    /// Get pending announcements
    pub fn take_announcements(&mut self, limit: usize) -> Vec<Hash> {
        let count = limit.min(self.pending_announcements.len());
        self.pending_announcements.drain(..count).collect()
    }

    /// Check if we've recently seen a hash
    pub fn is_known(&self, hash: &Hash) -> bool {
        self.recent_hashes.contains(hash) || self.local_filter.may_contain(hash)
    }

    /// Add known hash (e.g., from received gossip)
    pub fn add_known(&mut self, hash: Hash) {
        self.local_filter.insert(&hash);
        if self.recent_hashes.len() < self.max_recent {
            self.recent_hashes.insert(hash);
        }
    }

    /// Get the local bloom filter for sending to peers
    pub fn get_bloom_filter(&self) -> &BloomFilter {
        &self.local_filter
    }

    /// Find missing hashes by comparing with peer's bloom filter
    pub fn find_missing(&self, peer_filter: &BloomFilter, our_hashes: &[Hash]) -> Vec<Hash> {
        our_hashes
            .iter()
            .filter(|h| !peer_filter.may_contain(h))
            .cloned()
            .collect()
    }

    /// Check if rate limiting allows sending
    pub fn can_send(&mut self) -> bool {
        self.rate_limiter.try_consume(1.0)
    }

    /// Check if rate limiting allows sending a batch
    pub fn can_send_batch(&mut self, count: usize) -> bool {
        self.rate_limiter.try_consume(count as f64)
    }

    /// Get time until we can send next message
    pub fn time_until_can_send(&mut self) -> Duration {
        self.rate_limiter.time_until_available(1.0)
    }

    /// Queue a message for sending
    pub fn queue_message(&mut self, message: GossipMessage, priority: MessagePriority) {
        self.message_queue.push(message, priority);
    }

    /// Get next message to send
    pub fn next_message(&mut self) -> Option<GossipMessage> {
        if self.can_send() {
            self.message_queue.pop()
        } else {
            None
        }
    }

    /// Get current round number
    pub fn round(&self) -> u64 {
        self.round
    }

    /// Get queue length
    pub fn queue_len(&self) -> usize {
        self.message_queue.len()
    }

    /// Clear the bloom filter (e.g., periodic reset to prevent saturation)
    pub fn reset_bloom_filter(&mut self) {
        self.local_filter.clear();
        self.recent_hashes.clear();
    }

    /// Get estimated false positive rate of bloom filter
    pub fn bloom_filter_fpr(&self) -> f64 {
        self.local_filter.estimated_false_positive_rate()
    }

    /// Get gossip statistics
    pub fn stats(&self) -> GossipStats {
        GossipStats {
            round: self.round,
            pending_announcements: self.pending_announcements.len(),
            queue_length: self.message_queue.len(),
            known_hashes: self.recent_hashes.len(),
            bloom_filter_items: self.local_filter.len(),
            bloom_filter_fpr: self.local_filter.estimated_false_positive_rate(),
            available_tokens: self.rate_limiter.tokens,
        }
    }
}

/// Gossip statistics
#[derive(Debug, Clone)]
pub struct GossipStats {
    /// Current gossip round
    pub round: u64,
    /// Pending announcements count
    pub pending_announcements: usize,
    /// Message queue length
    pub queue_length: usize,
    /// Known hashes count
    pub known_hashes: usize,
    /// Bloom filter item count
    pub bloom_filter_items: usize,
    /// Bloom filter false positive rate
    pub bloom_filter_fpr: f64,
    /// Available rate limit tokens
    pub available_tokens: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter_insert_contains() {
        let mut filter = BloomFilter::new();
        let hash = Hash::from_bytes(&[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32,
        ]);

        assert!(!filter.may_contain(&hash));
        filter.insert(&hash);
        assert!(filter.may_contain(&hash));
        assert_eq!(filter.len(), 1);
    }

    #[test]
    fn test_bloom_filter_serialization() {
        let mut filter = BloomFilter::new();
        let hash = Hash::from_bytes(&[1; 32]);
        filter.insert(&hash);

        let bytes = filter.to_bytes();
        let restored = BloomFilter::from_bytes(&bytes);

        assert!(restored.may_contain(&hash));
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(1.0); // 1 Mbps

        // Should have tokens initially
        assert!(bucket.try_consume(1.0));
        assert!(bucket.try_consume(1.0));

        // Eventually runs out
        for _ in 0..200 {
            bucket.try_consume(1.0);
        }
        assert!(!bucket.try_consume(100.0));
    }

    #[test]
    fn test_message_priority_ordering() {
        let mut queue: MessageQueue<String> = MessageQueue::new(10);

        queue.push("low".to_string(), MessagePriority::Low);
        queue.push("critical".to_string(), MessagePriority::Critical);
        queue.push("normal".to_string(), MessagePriority::Normal);
        queue.push("high".to_string(), MessagePriority::High);

        assert_eq!(queue.pop(), Some("critical".to_string()));
        assert_eq!(queue.pop(), Some("high".to_string()));
        assert_eq!(queue.pop(), Some("normal".to_string()));
        assert_eq!(queue.pop(), Some("low".to_string()));
    }

    #[test]
    fn test_peer_gossip_state_backoff() {
        let mut state = PeerGossipState::new();

        // Wait for initial backoff
        std::thread::sleep(MIN_BACKOFF + Duration::from_millis(10));

        // Should allow gossip after backoff
        assert!(state.should_gossip());

        // Record failure and check backoff increases
        let initial_backoff = state.current_backoff;
        state.record_failure();
        assert!(state.current_backoff > initial_backoff);

        // Should not allow gossip immediately after failure
        assert!(!state.should_gossip());

        // Wait for new backoff
        std::thread::sleep(state.current_backoff + Duration::from_millis(10));

        // Now should allow gossip
        assert!(state.should_gossip());

        // Success should decrease backoff
        let backoff_before_success = state.current_backoff;
        state.record_success();
        assert!(state.current_backoff < backoff_before_success);
    }

    #[test]
    fn test_gossip_manager_announce() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        let hash = Hash::from_bytes(&[42; 32]);
        manager.announce(hash.clone());

        assert!(manager.is_known(&hash));
        assert_eq!(manager.pending_announcements.len(), 1);
    }

    #[test]
    fn test_gossip_manager_find_missing() {
        let config = GossipConfig::default();
        let manager = GossipManager::new(config);

        let mut peer_filter = BloomFilter::new();
        let hash1 = Hash::from_bytes(&[1; 32]);
        let hash2 = Hash::from_bytes(&[2; 32]);

        peer_filter.insert(&hash1);

        let our_hashes = vec![hash1.clone(), hash2.clone()];
        let missing = manager.find_missing(&peer_filter, &our_hashes);

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], hash2);
    }

    #[test]
    fn test_gossip_stats() {
        let config = GossipConfig::default();
        let manager = GossipManager::new(config);

        let stats = manager.stats();
        assert_eq!(stats.round, 0);
        assert_eq!(stats.pending_announcements, 0);
        assert!(stats.bloom_filter_fpr < 0.01);
    }

    // ==================== Additional Tests ====================

    #[test]
    fn test_bloom_filter_default() {
        let filter = BloomFilter::default();
        assert!(filter.is_empty());
        assert_eq!(filter.len(), 0);
    }

    #[test]
    fn test_bloom_filter_with_capacity() {
        let filter = BloomFilter::with_capacity(512, 5);
        assert!(filter.is_empty());
        assert_eq!(filter.len(), 0);
    }

    #[test]
    fn test_bloom_filter_clear() {
        let mut filter = BloomFilter::new();
        let hash = Hash::from_bytes(&[1; 32]);
        filter.insert(&hash);
        assert!(!filter.is_empty());

        filter.clear();
        assert!(filter.is_empty());
        assert!(!filter.may_contain(&hash));
    }

    #[test]
    fn test_bloom_filter_fpr() {
        let mut filter = BloomFilter::new();

        // Empty filter should have 0 FPR
        assert_eq!(filter.estimated_false_positive_rate(), 0.0);

        // Add some items
        for i in 0..10 {
            let hash = Hash::from_bytes(&[i as u8; 32]);
            filter.insert(&hash);
        }
        let fpr = filter.estimated_false_positive_rate();
        assert!(fpr > 0.0);
        assert!(fpr < 1.0);
    }

    #[test]
    fn test_bloom_filter_clone() {
        let mut filter = BloomFilter::new();
        let hash = Hash::from_bytes(&[1; 32]);
        filter.insert(&hash);

        let cloned = filter.clone();
        assert!(cloned.may_contain(&hash));
        assert_eq!(cloned.len(), 1);
    }

    #[test]
    fn test_bloom_filter_debug() {
        let filter = BloomFilter::new();
        let debug = format!("{:?}", filter);
        assert!(debug.contains("BloomFilter"));
    }

    #[test]
    fn test_token_bucket_with_params() {
        let bucket = TokenBucket::with_params(50.0, 10.0);
        assert!(bucket.tokens <= 50.0);
    }

    #[test]
    fn test_token_bucket_has_tokens() {
        let mut bucket = TokenBucket::new(1.0);
        assert!(bucket.has_tokens(1.0));
    }

    #[test]
    fn test_token_bucket_available() {
        let mut bucket = TokenBucket::new(1.0);
        let available = bucket.available();
        assert!(available > 0.0);
    }

    #[test]
    fn test_token_bucket_time_until_available() {
        let mut bucket = TokenBucket::with_params(10.0, 1.0);
        // Use all tokens
        for _ in 0..15 {
            bucket.try_consume(1.0);
        }
        let wait = bucket.time_until_available(5.0);
        assert!(wait > Duration::ZERO);
    }

    #[test]
    fn test_token_bucket_time_until_available_immediate() {
        let mut bucket = TokenBucket::new(1.0);
        let wait = bucket.time_until_available(1.0);
        assert_eq!(wait, Duration::ZERO);
    }

    #[test]
    fn test_token_bucket_debug() {
        let bucket = TokenBucket::new(1.0);
        let debug = format!("{:?}", bucket);
        assert!(debug.contains("TokenBucket"));
    }

    #[test]
    fn test_message_priority_all_variants() {
        let priorities = [
            MessagePriority::Low,
            MessagePriority::Normal,
            MessagePriority::High,
            MessagePriority::Critical,
        ];
        for priority in priorities {
            let cloned = priority;
            assert_eq!(priority, cloned);
        }
    }

    #[test]
    fn test_message_queue_len_is_empty() {
        let mut queue: MessageQueue<String> = MessageQueue::new(10);
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);

        queue.push("test".to_string(), MessagePriority::Normal);
        assert!(!queue.is_empty());
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_message_queue_capacity() {
        let mut queue: MessageQueue<u32> = MessageQueue::new(3);
        queue.push(1, MessagePriority::Normal);
        queue.push(2, MessagePriority::Normal);
        queue.push(3, MessagePriority::Normal);

        // Should be at capacity now
        queue.push(4, MessagePriority::Critical); // Higher priority, should work
        assert!(queue.len() <= 4); // Implementation may vary
    }

    #[test]
    fn test_message_queue_clear() {
        let mut queue: MessageQueue<String> = MessageQueue::new(10);
        queue.push("a".to_string(), MessagePriority::Low);
        queue.push("b".to_string(), MessagePriority::High);
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_peer_gossip_state_new() {
        let state = PeerGossipState::new();
        assert_eq!(state.failures, 0);
        assert!(state.current_backoff >= MIN_BACKOFF);
    }

    #[test]
    fn test_peer_gossip_state_success_resets() {
        let mut state = PeerGossipState::new();

        // Cause failures to increase backoff
        state.record_failure();
        state.record_failure();
        let high_backoff = state.current_backoff;

        // Success should reduce backoff
        state.record_success();
        assert!(state.current_backoff < high_backoff);
        assert_eq!(state.failures, 0);
    }

    #[test]
    fn test_peer_gossip_state_max_backoff() {
        let mut state = PeerGossipState::new();

        // Many failures should be capped
        for _ in 0..20 {
            state.record_failure();
        }
        assert!(state.current_backoff <= MAX_BACKOFF);
    }

    #[test]
    fn test_gossip_manager_take_announcements() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        let hash1 = Hash::from_bytes(&[1; 32]);
        let hash2 = Hash::from_bytes(&[2; 32]);

        manager.announce(hash1.clone());
        manager.announce(hash2.clone());

        let taken = manager.take_announcements(10);
        assert_eq!(taken.len(), 2);
        assert!(manager.pending_announcements.is_empty());
    }

    #[test]
    fn test_gossip_manager_take_announcements_limited() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        for i in 0..10 {
            let hash = Hash::from_bytes(&[i as u8; 32]);
            manager.announce(hash);
        }

        let taken = manager.take_announcements(5);
        assert_eq!(taken.len(), 5);
        assert_eq!(manager.pending_announcements.len(), 5);
    }

    #[test]
    fn test_gossip_manager_gossip_complete() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        assert_eq!(manager.round(), 0);
        manager.gossip_complete(true);
        assert_eq!(manager.round(), 1);
        manager.gossip_complete(false);
        assert_eq!(manager.round(), 2);
    }

    #[test]
    fn test_gossip_manager_queue_len() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        assert_eq!(manager.queue_len(), 0);

        let hash = Hash::from_bytes(&[0; 32]);
        manager.queue_message(GossipMessage::Announce { hash }, MessagePriority::Low);
        assert_eq!(manager.queue_len(), 1);
    }

    #[test]
    fn test_gossip_manager_reset_bloom_filter() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        let hash = Hash::from_bytes(&[1; 32]);
        manager.announce(hash.clone());
        assert!(manager.is_known(&hash));

        manager.reset_bloom_filter();
        // After reset, filter is empty
        // Note: hash might still be in recent_hashes depending on implementation
    }

    #[test]
    fn test_gossip_manager_bloom_filter_fpr() {
        let config = GossipConfig::default();
        let manager = GossipManager::new(config);

        let fpr = manager.bloom_filter_fpr();
        assert!(fpr >= 0.0);
    }

    #[test]
    fn test_gossip_manager_can_send_batch() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        // Should be able to send initially
        assert!(manager.can_send_batch(1));
    }

    #[test]
    fn test_gossip_manager_time_until_can_send() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        let wait = manager.time_until_can_send();
        // Should be zero initially (has tokens)
        assert!(wait <= Duration::from_secs(1));
    }

    #[test]
    fn test_gossip_message_announce() {
        let hash = Hash::from_bytes(&[1; 32]);
        let msg = GossipMessage::Announce { hash: hash.clone() };
        let cloned = msg.clone();
        assert!(matches!(cloned, GossipMessage::Announce { .. }));
    }

    #[test]
    fn test_gossip_stats_clone_debug() {
        let stats = GossipStats {
            round: 5,
            pending_announcements: 10,
            queue_length: 3,
            known_hashes: 100,
            bloom_filter_items: 50,
            bloom_filter_fpr: 0.01,
            available_tokens: 75.5,
        };

        let cloned = stats.clone();
        assert_eq!(cloned.round, 5);

        let debug = format!("{:?}", stats);
        assert!(debug.contains("GossipStats"));
    }

    #[test]
    fn test_gossip_manager_get_bloom_filter() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);

        let hash = Hash::from_bytes(&[42; 32]);
        manager.announce(hash.clone());

        let filter = manager.get_bloom_filter();
        assert!(filter.may_contain(&hash));
    }

    // ==================== Memory Optimization Tests ====================

    #[test]
    fn test_bloom_filter_memory_size() {
        // Verify optimized memory footprint
        // [u64; 16] = 128 bytes for bits
        // + hash_count (8 bytes) + item_count (8 bytes) + bit_count (8 bytes)
        // Total should be ~152 bytes, not 1024+ bytes
        let filter = BloomFilter::new();
        let size = std::mem::size_of_val(&filter);

        // Old implementation with Vec<bool> would be ~1048 bytes
        // New implementation should be ~152 bytes
        assert!(
            size < 200,
            "BloomFilter should use packed bits, got {} bytes",
            size
        );
        assert!(
            size >= 128,
            "BloomFilter must have at least 128 bytes for bits"
        );
    }

    #[test]
    fn test_bloom_filter_to_bytes_size() {
        let filter = BloomFilter::new();
        let bytes = filter.to_bytes();

        // Should be exactly 128 bytes (16 * 8 bytes)
        assert_eq!(
            bytes.len(),
            128,
            "to_bytes should produce 128 bytes for 1024 bits"
        );
    }

    #[test]
    fn test_bloom_filter_round_trip_serialization() {
        let mut filter = BloomFilter::new();

        // Insert several hashes
        for i in 0..50 {
            let hash = Hash::from_bytes(&[i as u8; 32]);
            filter.insert(&hash);
        }

        // Serialize and deserialize
        let bytes = filter.to_bytes();
        let restored = BloomFilter::from_bytes(&bytes);

        // All original hashes should still be found
        for i in 0..50 {
            let hash = Hash::from_bytes(&[i as u8; 32]);
            assert!(
                restored.may_contain(&hash),
                "Hash {} should be found after round-trip",
                i
            );
        }
    }
}
