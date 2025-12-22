//! Merkle tree for membership proofs
//!
//! Allows proving that an element is part of a set without revealing
//! the entire set.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Result, ZkError};

/// Hash type for Merkle tree nodes
pub type Hash = [u8; 32];

/// Merkle tree for set membership proofs
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// All nodes in the tree (leaves + internal nodes)
    nodes: Vec<Hash>,
    /// Number of leaves
    leaf_count: usize,
}

impl MerkleTree {
    /// Create a new Merkle tree from leaves
    pub fn new(leaves: &[&[u8]]) -> Result<Self> {
        if leaves.is_empty() {
            return Err(ZkError::EmptyTree);
        }

        // Hash all leaves
        let mut leaf_hashes: Vec<Hash> = leaves.iter().map(|leaf| Self::hash_leaf(leaf)).collect();

        let leaf_count = leaf_hashes.len();

        // Pad to power of 2
        let target_size = leaf_count.next_power_of_two();
        while leaf_hashes.len() < target_size {
            leaf_hashes.push([0u8; 32]); // Empty leaf hash
        }

        // Build tree bottom-up
        let mut nodes = leaf_hashes;
        let mut level_start = 0;
        let mut level_size = nodes.len();

        while level_size > 1 {
            let mut next_level = Vec::with_capacity(level_size / 2);
            for i in (0..level_size).step_by(2) {
                let left = &nodes[level_start + i];
                let right = &nodes[level_start + i + 1];
                next_level.push(Self::hash_internal(left, right));
            }
            level_start = nodes.len();
            level_size = next_level.len();
            nodes.extend(next_level);
        }

        Ok(Self { nodes, leaf_count })
    }

    /// Create from raw hashes (already hashed leaves)
    pub fn from_hashes(hashes: Vec<Hash>) -> Result<Self> {
        if hashes.is_empty() {
            return Err(ZkError::EmptyTree);
        }

        let leaf_count = hashes.len();
        let mut nodes = hashes;

        // Pad to power of 2
        let target_size = leaf_count.next_power_of_two();
        while nodes.len() < target_size {
            nodes.push([0u8; 32]);
        }

        // Build tree bottom-up
        let mut level_start = 0;
        let mut level_size = nodes.len();

        while level_size > 1 {
            let mut next_level = Vec::with_capacity(level_size / 2);
            for i in (0..level_size).step_by(2) {
                let left = &nodes[level_start + i];
                let right = &nodes[level_start + i + 1];
                next_level.push(Self::hash_internal(left, right));
            }
            level_start = nodes.len();
            level_size = next_level.len();
            nodes.extend(next_level);
        }

        Ok(Self { nodes, leaf_count })
    }

    /// Hash a leaf (with domain separator)
    fn hash_leaf(data: &[u8]) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update([0x00]); // Leaf prefix
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Hash two internal nodes
    fn hash_internal(left: &Hash, right: &Hash) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update([0x01]); // Internal node prefix
        hasher.update(left);
        hasher.update(right);
        hasher.finalize().into()
    }

    /// Get the root hash
    pub fn root(&self) -> Hash {
        *self.nodes.last().unwrap_or(&[0u8; 32])
    }

    /// Get the number of leaves
    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }

    /// Generate a proof for a leaf at the given index
    pub fn prove(&self, index: usize) -> Result<MerkleProof> {
        if index >= self.leaf_count {
            return Err(ZkError::LeafNotFound);
        }

        let padded_size = self.leaf_count.next_power_of_two();
        let mut proof_nodes = Vec::new();
        let mut current_index = index;
        let mut level_start = 0;
        let mut level_size = padded_size;

        while level_size > 1 {
            // Sibling index
            let sibling_index = if current_index % 2 == 0 {
                current_index + 1
            } else {
                current_index - 1
            };

            let sibling = self.nodes[level_start + sibling_index];
            let is_left = current_index % 2 == 1; // Is sibling on the left?

            proof_nodes.push(ProofNode {
                hash: sibling,
                is_left,
            });

            // Move to parent
            current_index /= 2;
            level_start += level_size;
            level_size /= 2;
        }

        Ok(MerkleProof {
            leaf_index: index,
            proof_nodes,
            root: self.root(),
        })
    }

    /// Generate a proof for specific data
    pub fn prove_data(&self, data: &[u8]) -> Result<MerkleProof> {
        let leaf_hash = Self::hash_leaf(data);
        let padded_size = self.leaf_count.next_power_of_two();

        // Find the leaf
        for i in 0..padded_size {
            if self.nodes[i] == leaf_hash {
                return self.prove(i);
            }
        }

        Err(ZkError::LeafNotFound)
    }
}

/// A node in the Merkle proof path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofNode {
    /// Hash of the sibling node
    pub hash: Hash,
    /// Whether this sibling is on the left
    pub is_left: bool,
}

/// Merkle inclusion proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    /// Index of the leaf being proved
    pub leaf_index: usize,
    /// Proof path from leaf to root
    pub proof_nodes: Vec<ProofNode>,
    /// Expected root hash
    pub root: Hash,
}

impl MerkleProof {
    /// Verify that data is included in the tree with this root
    pub fn verify(&self, data: &[u8]) -> bool {
        let mut current_hash = MerkleTree::hash_leaf(data);

        for node in &self.proof_nodes {
            current_hash = if node.is_left {
                MerkleTree::hash_internal(&node.hash, &current_hash)
            } else {
                MerkleTree::hash_internal(&current_hash, &node.hash)
            };
        }

        current_hash == self.root
    }

    /// Verify with an already-hashed leaf
    pub fn verify_hash(&self, leaf_hash: &Hash) -> bool {
        // Need to add leaf prefix since hash was computed without it
        let mut hasher = sha2::Sha256::new();
        hasher.update([0x00]); // Leaf prefix
        hasher.update(leaf_hash);
        let mut current_hash: Hash = hasher.finalize().into();

        for node in &self.proof_nodes {
            current_hash = if node.is_left {
                MerkleTree::hash_internal(&node.hash, &current_hash)
            } else {
                MerkleTree::hash_internal(&current_hash, &node.hash)
            };
        }

        current_hash == self.root
    }

    /// Get the proof size in bytes
    pub fn size(&self) -> usize {
        self.proof_nodes.len() * 33 + 32 // 32 bytes hash + 1 byte flag per node, plus root
    }

    /// Get proof as hex-encoded string for transmission
    pub fn to_hex(&self) -> String {
        let mut result = hex::encode(self.root);
        for node in &self.proof_nodes {
            result.push_str(&hex::encode(node.hash));
            result.push(if node.is_left { 'L' } else { 'R' });
        }
        result
    }
}

/// Node key for internal node cache (depth, path_bits_as_hash)
type NodeKey = (usize, Hash);

/// Sparse Merkle tree for efficient membership proofs with non-membership
#[derive(Debug, Clone)]
pub struct SparseMerkleTree {
    /// Height of the tree (256 for full SHA256 key space)
    height: usize,
    /// Cached root hash
    root: Hash,
    /// Non-empty leaf nodes (key_hash -> value_hash)
    leaves: std::collections::HashMap<Hash, Hash>,
    /// Cached internal nodes (depth, path_prefix) -> hash
    nodes: std::collections::HashMap<NodeKey, Hash>,
    /// Default hashes for each level (empty subtree hashes)
    default_hashes: Vec<Hash>,
}

/// Sparse Merkle tree inclusion/exclusion proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseMerkleProof {
    /// Key being proved
    pub key: Hash,
    /// Value hash (None for non-membership proof)
    pub value: Option<Hash>,
    /// Sibling hashes along the path from leaf to root
    pub siblings: Vec<Hash>,
    /// Expected root hash
    pub root: Hash,
}

impl SparseMerkleTree {
    /// Create a new sparse Merkle tree with default height of 256
    pub fn new() -> Self {
        Self::with_height(256)
    }

    /// Create a new sparse Merkle tree with specified height
    pub fn with_height(height: usize) -> Self {
        let default_hashes = Self::compute_default_hashes(height);
        let root = default_hashes[height];

        Self {
            height,
            root,
            leaves: std::collections::HashMap::new(),
            nodes: std::collections::HashMap::new(),
            default_hashes,
        }
    }

    /// Compute default hashes for empty subtrees at each level
    /// default_hashes[0] = hash(empty) = [0; 32]
    /// default_hashes[i] = hash(default_hashes[i-1] || default_hashes[i-1])
    fn compute_default_hashes(height: usize) -> Vec<Hash> {
        let mut default_hashes = vec![[0u8; 32]; height + 1];

        for i in 1..=height {
            default_hashes[i] =
                MerkleTree::hash_internal(&default_hashes[i - 1], &default_hashes[i - 1]);
        }

        default_hashes
    }

    /// Insert a key-value pair and return the new root
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<Hash> {
        let key_hash = Self::hash_key(key);
        let value_hash = MerkleTree::hash_leaf(value);

        self.leaves.insert(key_hash, value_hash);

        // Clear cache and recompute root
        self.nodes.clear();
        self.recompute_root();

        Ok(self.root)
    }

    /// Get the value hash for a key
    pub fn get(&self, key: &[u8]) -> Option<Hash> {
        let key_hash = Self::hash_key(key);
        self.leaves.get(&key_hash).copied()
    }

    /// Check if a key exists
    pub fn contains(&self, key: &[u8]) -> bool {
        let key_hash = Self::hash_key(key);
        self.leaves.contains_key(&key_hash)
    }

    /// Delete a key and return the old value hash
    pub fn delete(&mut self, key: &[u8]) -> Option<Hash> {
        let key_hash = Self::hash_key(key);

        if let Some(old_value) = self.leaves.remove(&key_hash) {
            // Clear cache and recompute root
            self.nodes.clear();
            self.recompute_root();
            Some(old_value)
        } else {
            None
        }
    }

    /// Get the root hash
    pub fn root(&self) -> Hash {
        self.root
    }

    /// Recompute the root hash from scratch
    fn recompute_root(&mut self) {
        if self.leaves.is_empty() {
            self.root = self.default_hashes[self.height];
            return;
        }

        // Compute root by traversing from top down
        let root_key = [0u8; 32]; // Root represents all bits as 0
        self.root = self.compute_subtree(&root_key, self.height);
    }

    /// Compute the hash of a subtree rooted at a given prefix and depth
    /// prefix contains the path bits from the root down to this level
    /// depth indicates how many levels below this node (0 = leaf)
    fn compute_subtree(&mut self, prefix: &Hash, depth: usize) -> Hash {
        if depth == 0 {
            // At leaf level
            return self
                .leaves
                .get(prefix)
                .copied()
                .unwrap_or(self.default_hashes[0]);
        }

        // Check cache first
        if let Some(&cached) = self.nodes.get(&(depth, *prefix)) {
            return cached;
        }

        // At depth d, we're (height - d) bits down from root
        // The bit index we're splitting on is (height - d)
        let bit_index = self.height - depth;

        // Build left and right child prefixes
        let left_prefix = *prefix; // Bit at bit_index is 0 (already 0)
        let mut right_prefix = *prefix;
        Self::set_bit_mut(&mut right_prefix, bit_index);

        // Compute hashes for left and right subtrees
        let left_hash = if depth == 1 {
            // Children are leaves
            self.leaves
                .get(&left_prefix)
                .copied()
                .unwrap_or(self.default_hashes[0])
        } else if self.has_leaf_with_prefix(&left_prefix, depth - 1) {
            self.compute_subtree(&left_prefix, depth - 1)
        } else {
            self.default_hashes[depth - 1]
        };

        let right_hash = if depth == 1 {
            // Children are leaves
            self.leaves
                .get(&right_prefix)
                .copied()
                .unwrap_or(self.default_hashes[0])
        } else if self.has_leaf_with_prefix(&right_prefix, depth - 1) {
            self.compute_subtree(&right_prefix, depth - 1)
        } else {
            self.default_hashes[depth - 1]
        };

        let hash = MerkleTree::hash_internal(&left_hash, &right_hash);

        // Cache this node
        self.nodes.insert((depth, *prefix), hash);

        hash
    }

    /// Check if any leaf exists with the given prefix
    /// At depth d, we check the first (height - d) bits
    fn has_leaf_with_prefix(&self, prefix: &Hash, depth: usize) -> bool {
        let bits_to_check = self.height - depth;

        for leaf_key in self.leaves.keys() {
            if self.matches_prefix(leaf_key, prefix, bits_to_check) {
                return true;
            }
        }
        false
    }

    /// Check if a key matches a prefix for the first `num_bits` bits
    fn matches_prefix(&self, key: &Hash, prefix: &Hash, num_bits: usize) -> bool {
        for i in 0..num_bits {
            if Self::get_bit(key, i) != Self::get_bit(prefix, i) {
                return false;
            }
        }
        true
    }

    /// Set a bit (helper)
    fn set_bit_mut(key: &mut Hash, index: usize) {
        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        key[byte_index] |= 1 << bit_index;
    }

    /// Generate a membership proof for a key
    pub fn prove(&self, key: &[u8]) -> Result<SparseMerkleProof> {
        let key_hash = Self::hash_key(key);
        let value = self.leaves.get(&key_hash).copied();

        if value.is_none() {
            return Err(ZkError::LeafNotFound);
        }

        let siblings = self.collect_siblings(&key_hash);

        Ok(SparseMerkleProof {
            key: key_hash,
            value,
            siblings,
            root: self.root,
        })
    }

    /// Generate a non-membership proof for a key
    pub fn prove_non_membership(&self, key: &[u8]) -> Result<SparseMerkleProof> {
        let key_hash = Self::hash_key(key);

        if self.leaves.contains_key(&key_hash) {
            return Err(ZkError::InvalidProof("Key exists".into()));
        }

        let siblings = self.collect_siblings(&key_hash);

        Ok(SparseMerkleProof {
            key: key_hash,
            value: None,
            siblings,
            root: self.root,
        })
    }

    /// Collect sibling hashes along the path from leaf to root
    /// This must match the order used in compute_subtree!
    fn collect_siblings(&self, key: &Hash) -> Vec<Hash> {
        let mut siblings = Vec::with_capacity(self.height);

        // Traverse from leaf (depth 0) to root (depth height)
        // At each level, we need the sibling of the current node
        for level in 0..self.height {
            // level 0 = leaves, level 1 = one above leaves, etc.
            // The bit index we're looking at is (height - 1 - level) from root down
            let bit_index = self.height - 1 - level;
            let bit = Self::get_bit(key, bit_index);

            // Build prefix up to this level
            let mut prefix = [0u8; 32];
            for i in 0..bit_index {
                if Self::get_bit(key, i) {
                    Self::set_bit_mut(&mut prefix, i);
                }
            }

            // Get sibling by flipping the bit at bit_index
            let mut sibling_prefix = prefix;
            if bit {
                // Current is right (bit=1), sibling is left (bit=0) - already 0
                sibling_prefix = prefix; // bit at bit_index is 0
            } else {
                // Current is left (bit=0), sibling is right (bit=1)
                Self::set_bit_mut(&mut sibling_prefix, bit_index);
            }

            // Get the hash for this sibling
            let sibling_hash = if level == 0 {
                // Sibling is a leaf
                self.leaves
                    .get(&sibling_prefix)
                    .copied()
                    .unwrap_or(self.default_hashes[0])
            } else {
                // Sibling is an internal node at depth `level`
                self.nodes
                    .get(&(level, sibling_prefix))
                    .copied()
                    .unwrap_or(self.default_hashes[level])
            };

            siblings.push(sibling_hash);
        }

        siblings
    }

    /// Verify a sparse Merkle proof
    pub fn verify_proof(proof: &SparseMerkleProof, expected_root: &Hash) -> bool {
        // Start with leaf hash
        let mut current_hash = proof.value.unwrap_or([0u8; 32]);

        // Traverse up the tree - siblings are ordered from leaf to root
        // Level 0 = leaves, increasing levels go towards root
        for (level, sibling) in proof.siblings.iter().enumerate() {
            // The bit index at this level (from root down)
            let height = proof.siblings.len(); // Should be 256
            let bit_index = height - 1 - level;
            let bit = Self::get_bit(&proof.key, bit_index);

            current_hash = if bit {
                // Current node is right child
                MerkleTree::hash_internal(sibling, &current_hash)
            } else {
                // Current node is left child
                MerkleTree::hash_internal(&current_hash, sibling)
            };
        }

        current_hash == *expected_root
    }

    /// Hash a key to get its position in the tree
    fn hash_key(key: &[u8]) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.finalize().into()
    }

    /// Get the bit at a given index in the key (0 = MSB)
    fn get_bit(key: &Hash, index: usize) -> bool {
        let byte_index = index / 8;
        let bit_index = 7 - (index % 8); // MSB first
        (key[byte_index] >> bit_index) & 1 == 1
    }
}

impl Default for SparseMerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_tree_single_leaf() {
        let leaves = vec![b"hello".as_slice()];
        let tree = MerkleTree::new(&leaves).unwrap();

        assert_eq!(tree.leaf_count(), 1);

        let proof = tree.prove(0).unwrap();
        assert!(proof.verify(b"hello"));
        assert!(!proof.verify(b"world"));
    }

    #[test]
    fn test_merkle_tree_multiple_leaves() {
        let leaves: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d"];
        let tree = MerkleTree::new(&leaves).unwrap();

        assert_eq!(tree.leaf_count(), 4);

        // Verify each leaf
        for (i, leaf) in leaves.iter().enumerate() {
            let proof = tree.prove(i).unwrap();
            assert!(proof.verify(leaf), "Failed to verify leaf {}", i);
        }
    }

    #[test]
    fn test_merkle_proof_by_data() {
        let leaves: Vec<&[u8]> = vec![b"alice", b"bob", b"charlie"];
        let tree = MerkleTree::new(&leaves).unwrap();

        let proof = tree.prove_data(b"bob").unwrap();
        assert!(proof.verify(b"bob"));

        // Non-existent data should fail
        let result = tree.prove_data(b"dave");
        assert!(result.is_err());
    }

    #[test]
    fn test_merkle_root_consistency() {
        let leaves1: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d"];
        let leaves2: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d"];
        let leaves3: Vec<&[u8]> = vec![b"a", b"b", b"c", b"e"]; // Different last leaf

        let tree1 = MerkleTree::new(&leaves1).unwrap();
        let tree2 = MerkleTree::new(&leaves2).unwrap();
        let tree3 = MerkleTree::new(&leaves3).unwrap();

        assert_eq!(tree1.root(), tree2.root());
        assert_ne!(tree1.root(), tree3.root());
    }

    #[test]
    fn test_sparse_merkle_tree() {
        let mut tree = SparseMerkleTree::new();

        assert!(!tree.contains(b"key1"));

        tree.insert(b"key1", b"value1").unwrap();
        assert!(tree.contains(b"key1"));
        assert!(!tree.contains(b"key2"));

        let root1 = tree.root();
        tree.insert(b"key2", b"value2").unwrap();
        let root2 = tree.root();

        assert_ne!(root1, root2);
    }

    #[test]
    fn test_sparse_merkle_insert_and_prove() {
        let mut tree = SparseMerkleTree::new();

        // Insert a key-value pair
        tree.insert(b"key1", b"value1").unwrap();

        // Generate membership proof
        let proof = tree.prove(b"key1").unwrap();

        // Verify proof
        assert!(SparseMerkleTree::verify_proof(&proof, &tree.root()));
        assert!(proof.value.is_some());
    }

    #[test]
    fn test_sparse_merkle_non_membership() {
        let mut tree = SparseMerkleTree::new();

        // Insert one key
        tree.insert(b"key1", b"value1").unwrap();

        // Generate non-membership proof for key2
        let proof = tree.prove_non_membership(b"key2").unwrap();

        // Verify non-membership proof
        assert!(SparseMerkleTree::verify_proof(&proof, &tree.root()));
        assert!(proof.value.is_none());
    }

    #[test]
    fn test_sparse_merkle_multiple_inserts() {
        let mut tree = SparseMerkleTree::new();

        // Insert multiple keys
        tree.insert(b"alice", b"value_a").unwrap();
        tree.insert(b"bob", b"value_b").unwrap();
        tree.insert(b"charlie", b"value_c").unwrap();

        // Verify all can be proved
        let proof_a = tree.prove(b"alice").unwrap();
        let proof_b = tree.prove(b"bob").unwrap();
        let proof_c = tree.prove(b"charlie").unwrap();

        let root = tree.root();
        assert!(SparseMerkleTree::verify_proof(&proof_a, &root));
        assert!(SparseMerkleTree::verify_proof(&proof_b, &root));
        assert!(SparseMerkleTree::verify_proof(&proof_c, &root));
    }

    #[test]
    fn test_sparse_merkle_delete() {
        let mut tree = SparseMerkleTree::new();

        // Insert and then delete
        tree.insert(b"key1", b"value1").unwrap();
        assert!(tree.contains(b"key1"));

        let old_value = tree.delete(b"key1");
        assert!(old_value.is_some());
        assert!(!tree.contains(b"key1"));

        // Non-existent key deletion should return None
        let result = tree.delete(b"key2");
        assert!(result.is_none());
    }

    #[test]
    fn test_sparse_merkle_get() {
        let mut tree = SparseMerkleTree::new();

        tree.insert(b"key1", b"value1").unwrap();

        let value = tree.get(b"key1");
        assert!(value.is_some());

        let no_value = tree.get(b"nonexistent");
        assert!(no_value.is_none());
    }

    #[test]
    fn test_sparse_merkle_root_changes() {
        let mut tree = SparseMerkleTree::new();

        let empty_root = tree.root();

        tree.insert(b"key1", b"value1").unwrap();
        let root1 = tree.root();
        assert_ne!(empty_root, root1);

        tree.insert(b"key2", b"value2").unwrap();
        let root2 = tree.root();
        assert_ne!(root1, root2);

        tree.delete(b"key1");
        let root3 = tree.root();
        assert_ne!(root2, root3);
    }

    #[test]
    fn test_sparse_merkle_proof_invalid_key() {
        let mut tree = SparseMerkleTree::new();

        tree.insert(b"key1", b"value1").unwrap();

        // Trying to prove non-existent key should fail
        let result = tree.prove(b"key2");
        assert!(result.is_err());
    }

    #[test]
    fn test_sparse_merkle_non_membership_existing_key() {
        let mut tree = SparseMerkleTree::new();

        tree.insert(b"key1", b"value1").unwrap();

        // Trying to prove non-membership of existing key should fail
        let result = tree.prove_non_membership(b"key1");
        assert!(result.is_err());
    }

    #[test]
    fn test_sparse_merkle_deterministic_root() {
        let mut tree1 = SparseMerkleTree::new();
        let mut tree2 = SparseMerkleTree::new();

        // Insert same keys in different order
        tree1.insert(b"a", b"value_a").unwrap();
        tree1.insert(b"b", b"value_b").unwrap();

        tree2.insert(b"b", b"value_b").unwrap();
        tree2.insert(b"a", b"value_a").unwrap();

        // Roots should be the same regardless of insertion order
        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_sparse_merkle_empty_tree() {
        let tree = SparseMerkleTree::new();
        let empty_root = tree.root();

        // Empty tree should have deterministic default root
        let tree2 = SparseMerkleTree::new();
        assert_eq!(empty_root, tree2.root());
    }

    #[test]
    fn test_sparse_merkle_proof_serialization() {
        let mut tree = SparseMerkleTree::new();
        tree.insert(b"key1", b"value1").unwrap();

        let proof = tree.prove(b"key1").unwrap();

        // Serialize and deserialize
        let json = serde_json::to_string(&proof).unwrap();
        let deserialized: SparseMerkleProof = serde_json::from_str(&json).unwrap();

        // Verify deserialized proof works
        assert!(SparseMerkleTree::verify_proof(&deserialized, &tree.root()));
    }

    #[test]
    fn test_proof_serialization() {
        let leaves: Vec<&[u8]> = vec![b"x", b"y", b"z"];
        let tree = MerkleTree::new(&leaves).unwrap();
        let proof = tree.prove(1).unwrap();

        let json = serde_json::to_string(&proof).unwrap();
        let deserialized: MerkleProof = serde_json::from_str(&json).unwrap();

        assert_eq!(proof.root, deserialized.root);
        assert!(deserialized.verify(b"y"));
    }
}
