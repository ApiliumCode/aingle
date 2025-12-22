//! Simple actor implementation to replace the missing GhostActor from ghost_actor crate.
//!
//! This provides a minimal actor implementation that:
//! - Holds inner state protected by a mutex
//! - Provides async invoke method to run closures on the state
//! - Has shutdown/is_active lifecycle management

use ghost_actor::GhostError;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A simple actor wrapper that holds state and provides async access to it.
pub struct GhostActor<T> {
    inner: Arc<Mutex<T>>,
    active: Arc<AtomicBool>,
}

// Methods available for all T
impl<T> GhostActor<T> {
    /// Check if the actor is still active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Shutdown the actor.
    pub fn shutdown(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

impl<T> std::fmt::Debug for GhostActor<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GhostActor")
            .field("active", &self.is_active())
            .finish()
    }
}

impl<T> Clone for GhostActor<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            active: self.active.clone(),
        }
    }
}

// Methods requiring Send + 'static
impl<T: Send + 'static> GhostActor<T> {
    /// Create a new GhostActor with the given inner state.
    /// Returns the actor handle and a driver future (which is a no-op in this simple impl).
    pub fn new(inner: T) -> (Self, impl std::future::Future<Output = ()>) {
        let actor = Self {
            inner: Arc::new(Mutex::new(inner)),
            active: Arc::new(AtomicBool::new(true)),
        };
        // Driver is a no-op future since we use synchronous mutex access
        let driver = async {};
        (actor, driver)
    }

    /// Invoke a closure on the inner state.
    pub async fn invoke<R, F>(&self, f: F) -> Result<R, GhostError>
    where
        F: FnOnce(&mut T) -> Result<R, GhostError> + Send + 'static,
        R: Send + 'static,
    {
        if !self.is_active() {
            return Err(GhostError::Disconnected);
        }
        let mut guard = self.inner.lock();
        f(&mut *guard)
    }
}
