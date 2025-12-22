//! User management and credential validation

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use std::collections::HashMap;
use std::sync::RwLock;

/// User record
#[derive(Clone, Debug)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub created_at: u64,
    pub active: bool,
}

/// User store (in-memory for now, can be replaced with DB)
pub struct UserStore {
    users: RwLock<HashMap<String, User>>,
}

impl UserStore {
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new user with hashed password
    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        roles: Vec<String>,
    ) -> Result<User, String> {
        let mut users = self.users.write().map_err(|e| e.to_string())?;

        if users.values().any(|u| u.username == username) {
            return Err("Username already exists".into());
        }

        let password_hash = self.hash_password(password)?;
        let id = uuid::Uuid::new_v4().to_string();

        let user = User {
            id: id.clone(),
            username: username.to_string(),
            password_hash,
            roles,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            active: true,
        };

        users.insert(id.clone(), user.clone());
        Ok(user)
    }

    /// Validate credentials and return user if valid
    pub fn validate_credentials(&self, username: &str, password: &str) -> Result<User, String> {
        let users = self.users.read().map_err(|e| e.to_string())?;

        let user = users
            .values()
            .find(|u| u.username == username && u.active)
            .ok_or("Invalid credentials")?;

        if self.verify_password(password, &user.password_hash)? {
            Ok(user.clone())
        } else {
            Err("Invalid credentials".into())
        }
    }

    /// Hash password using argon2
    fn hash_password(&self, password: &str) -> Result<String, String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| e.to_string())?;
        Ok(password_hash.to_string())
    }

    /// Verify password against hash
    fn verify_password(&self, password: &str, hash: &str) -> Result<bool, String> {
        let parsed_hash = PasswordHash::new(hash).map_err(|e| e.to_string())?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Get user by ID
    pub fn get_user(&self, id: &str) -> Option<User> {
        self.users.read().ok()?.get(id).cloned()
    }

    /// Get user by username
    pub fn get_user_by_username(&self, username: &str) -> Option<User> {
        self.users
            .read()
            .ok()?
            .values()
            .find(|u| u.username == username)
            .cloned()
    }

    /// Deactivate user
    pub fn deactivate_user(&self, id: &str) -> Result<(), String> {
        let mut users = self.users.write().map_err(|e| e.to_string())?;
        if let Some(user) = users.get_mut(id) {
            user.active = false;
            Ok(())
        } else {
            Err("User not found".into())
        }
    }

    /// Initialize with default admin user
    pub fn init_default_admin(&self) -> Result<User, String> {
        self.create_user("admin", "admin123", vec!["admin".into(), "user".into()])
    }
}

impl Default for UserStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation_and_validation() {
        let store = UserStore::new();

        let user = store
            .create_user("testuser", "password123", vec!["user".into()])
            .unwrap();

        assert_eq!(user.username, "testuser");

        let validated = store
            .validate_credentials("testuser", "password123")
            .unwrap();
        assert_eq!(validated.id, user.id);

        let invalid = store.validate_credentials("testuser", "wrongpassword");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_duplicate_username() {
        let store = UserStore::new();

        store
            .create_user("testuser", "password123", vec!["user".into()])
            .unwrap();

        let duplicate = store.create_user("testuser", "password456", vec!["user".into()]);
        assert!(duplicate.is_err());
        assert_eq!(duplicate.unwrap_err(), "Username already exists");
    }

    #[test]
    fn test_password_hashing() {
        let store = UserStore::new();

        let hash1 = store.hash_password("password123").unwrap();
        let hash2 = store.hash_password("password123").unwrap();

        // Same password should produce different hashes (due to random salt)
        assert_ne!(hash1, hash2);

        // Both hashes should verify correctly
        assert!(store.verify_password("password123", &hash1).unwrap());
        assert!(store.verify_password("password123", &hash2).unwrap());
    }

    #[test]
    fn test_user_deactivation() {
        let store = UserStore::new();

        let user = store
            .create_user("testuser", "password123", vec!["user".into()])
            .unwrap();

        assert!(store
            .validate_credentials("testuser", "password123")
            .is_ok());

        store.deactivate_user(&user.id).unwrap();

        let result = store.validate_credentials("testuser", "password123");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_user_by_username() {
        let store = UserStore::new();

        let user = store
            .create_user("testuser", "password123", vec!["user".into()])
            .unwrap();

        let found = store.get_user_by_username("testuser");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, user.id);

        let not_found = store.get_user_by_username("nonexistent");
        assert!(not_found.is_none());
    }
}
