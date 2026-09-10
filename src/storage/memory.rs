//! In-process credential store.

use std::sync::RwLock;

use crate::auth::Session;
use crate::error::StorageError;

use super::CredentialStore;

/// An in-memory [`CredentialStore`] that holds at most one [`Session`] for the lifetime of the
/// process.
///
/// Ported from `MemoryStorage` (`auth_storage.py:240-261`): `load()` returns
/// [`Session::empty`] until the first [`MemoryStore::save`], `save()` replaces whatever was
/// held, and `clear()` resets to empty. This is the default backend when a caller wants no
/// persistence at all (`create_storage(use_keyring=False, cookie_file=None)` in Python,
/// `auth_storage.py:318-344`, always yields a `MemoryStorage`).
///
/// One behaviour is intentionally *not* carried over: Python's `load()` returns the very same
/// `AuthCredentials` object reference every time (`auth_storage.py:253`), so a caller that
/// mutates the returned value in place mutates the store's internal state without ever calling
/// `save()` — a latent footgun the brief calls out explicitly. Rust's ownership model makes
/// this reproduction impossible by construction: [`CredentialStore::load`] returns an owned
/// [`Session`], so [`MemoryStore::load`] always returns a clone, and mutating it can never
/// affect the store. This is a strict safety improvement, not a behavioural gap.
#[derive(Debug, Default)]
pub struct MemoryStore(RwLock<Option<Session>>);

impl MemoryStore {
    /// Creates an empty store, equivalent to `MemoryStorage()`'s fresh `AuthCredentials()`
    /// (`auth_storage.py:249`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Maps a poisoned lock (only reachable if a prior holder panicked while holding it) to a
    /// [`StorageError::Backend`] instead of propagating the panic, since nothing in this
    /// crate's own code ever panics while holding the lock.
    fn poisoned() -> StorageError {
        StorageError::Backend {
            backend: "memory".to_owned(),
            message: "lock poisoned by a prior panic".to_owned(),
        }
    }
}

impl CredentialStore for MemoryStore {
    fn load(&self) -> Result<Session, StorageError> {
        let guard = self.0.read().map_err(|_| Self::poisoned())?;
        Ok(guard.clone().unwrap_or_else(Session::empty))
    }

    fn save(&self, session: &Session) -> Result<(), StorageError> {
        let mut guard = self.0.write().map_err(|_| Self::poisoned())?;
        *guard = Some(session.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), StorageError> {
        let mut guard = self.0.write().map_err(|_| Self::poisoned())?;
        *guard = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialStore, MemoryStore};
    use crate::auth::Session;

    #[test]
    fn load_before_save_returns_empty_session() {
        let store = MemoryStore::new();
        let session = store.load().expect("load never fails");
        assert!(!session.is_valid());
        assert_eq!(session.expose_token(), "");
    }

    #[test]
    fn save_then_load_round_trips_the_session() {
        let store = MemoryStore::new();
        let session = Session::from_token("tok-123");
        store.save(&session).expect("save succeeds");

        let loaded = store.load().expect("load succeeds");
        assert_eq!(loaded.expose_token(), "tok-123");
        assert!(loaded.is_valid());
    }

    #[test]
    fn save_replaces_the_previously_stored_session() {
        let store = MemoryStore::new();
        store
            .save(&Session::from_token("first"))
            .expect("save succeeds");
        store
            .save(&Session::from_token("second"))
            .expect("save succeeds");

        let loaded = store.load().expect("load succeeds");
        assert_eq!(loaded.expose_token(), "second");
    }

    #[test]
    fn clear_resets_to_an_empty_session() {
        let store = MemoryStore::new();
        store
            .save(&Session::from_token("tok-123"))
            .expect("save succeeds");

        store.clear().expect("clear succeeds");

        let loaded = store.load().expect("load succeeds");
        assert!(!loaded.is_valid());
        assert_eq!(loaded.expose_token(), "");
    }

    #[test]
    fn clear_before_any_save_is_a_no_op() {
        let store = MemoryStore::new();
        store.clear().expect("clear never fails, even when empty");
        assert!(!store.load().expect("load succeeds").is_valid());
    }

    #[test]
    fn each_load_returns_an_independent_clone() {
        // Documents the deliberate behavioural improvement over `MemoryStorage.load()`, which
        // returns the very same live object reference on every call in Python
        // (`auth_storage.py:253`). Here, `load()` returns an owned `Session`, so dropping it
        // (or, in a real caller, ever mutating a local copy through a future setter) cannot
        // reach back into the store — only `save()` can change what is stored.
        let store = MemoryStore::new();
        store
            .save(&Session::from_token("original"))
            .expect("save succeeds");

        let first_load = store.load().expect("load succeeds");
        drop(first_load);

        assert_eq!(
            store.load().expect("load succeeds").expose_token(),
            "original"
        );
    }
}
