//! Pluggable credential storage.
//!
//! Ported from `eero-api`'s `src/eero/api/auth_storage.py`. See
//! `rusteero-context/claude/tasks/briefs/const.md` §3 for the full behaviour brief of the
//! `CredentialStorage` abstract base class this trait mirrors, and the port plan §3.4
//! (decisions D-4, D-5, D-13) for the design rationale.
//!
//! # Backends
//!
//! - [`memory::MemoryStore`] — in this file's sibling module, ported from `MemoryStorage`
//!   (`auth_storage.py:240-261`).
//! - `FileStore`, `ChainedStore`, and `KeyringStore` (behind `feature = "keyring"`) are added by
//!   a later phase of the port; see the "Phase 2" marker below for where they extend this
//!   module.

pub mod memory;

pub use crate::error::StorageError;
pub use memory::MemoryStore;

use crate::auth::Session;

/// A pluggable backend for persisting a [`Session`] across process restarts.
///
/// Mirrors the `CredentialStorage` abstract base class (`auth_storage.py:94-118`), which
/// declares three `async` abstract methods (`load`, `save`, `clear`) with no default bodies.
///
/// This trait is deliberately **synchronous**, not `async`, even though every method it
/// mirrors is `async def` in Python (port plan §3.4): every native OS keyring API
/// (`keyring` crate, `feature = "keyring"`) is a blocking call, so an async trait here would
/// only move the blocking work around, not remove it, while forcing `async-trait` boxing on
/// every implementation for no benefit. Async callers (the not-yet-ported `transport`/`Client`
/// layer) are expected to invoke these methods via `tokio::task::spawn_blocking`.
///
/// Implementations must be `Send + Sync` (usable from any task/thread) and `Debug` (so a
/// `Client` holding one behind an `Arc<dyn CredentialStore>` can still derive/implement
/// `Debug` itself).
pub trait CredentialStore: Send + Sync + std::fmt::Debug {
    /// Loads the stored session.
    ///
    /// Returns `Ok(`[`Session::empty`]`())`, **not** an error, when nothing has ever been
    /// stored — mirroring every Python backend's "never seen before" behaviour (e.g.
    /// `MemoryStorage.__init__`'s fresh `AuthCredentials()`, `auth_storage.py:249`, or
    /// `FileStorage.load()`'s "file not found" branch, `auth_storage.py:188-190`).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] only when the backend itself is unusable (e.g. an I/O failure
    /// reading a file, or a keyring backend reporting an error) — never merely because nothing
    /// has been saved yet. See decision D-13 for how a `Client` builder is expected to treat a
    /// failure here (warn by default, fatal opt-in).
    fn load(&self) -> Result<Session, StorageError>;

    /// Persists `session`, overwriting whatever was previously stored.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the backend could not persist the session (e.g. a file
    /// system permission failure). Unlike every Python backend (which swallows save failures
    /// internally and logs at DEBUG/ERROR, `auth_storage.py:147-155,211-228`), this port
    /// surfaces the failure as a typed error so a caller can decide whether it is fatal.
    fn save(&self, session: &Session) -> Result<(), StorageError>;

    /// Removes the stored session entirely.
    ///
    /// Mirrors `clear()` (e.g. `auth_storage.py:157-166,230-237,259-261`): unlike
    /// [`CredentialStore::save`] with an [`Session::empty`], this removes the underlying
    /// storage entry/file rather than overwriting it with null fields.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the backend failed to remove the stored entry. Removing an
    /// entry that was never there is *not* an error (mirrors every Python backend's tolerant
    /// `clear()`, e.g. the swallowed `PasswordDeleteError` at `auth_storage.py:162-164`).
    fn clear(&self) -> Result<(), StorageError>;
}

// Phase 2 extends this module with:
//   - `FileStore`, `ChainedStore`, and `KeyringStore` (`feature = "keyring"`) modules, declared
//     alongside `pub mod memory;` above.
//   - An async adapter that runs any `Arc<dyn CredentialStore>` via
//     `tokio::task::spawn_blocking` for use from `transport`/`Client`.
//   - `create_storage(...)`, a factory mirroring `auth_storage.py:318-344`'s matrix (both →
//     `ChainedStore`; keyring only; file only; neither → `MemoryStore`).
