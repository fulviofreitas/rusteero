//! Pluggable credential storage.
//!
//! Ported from `eero-api`'s `src/eero/api/auth_storage.py`. See
//! the const behaviour notes §3-8 for the full behaviour brief of the
//! `CredentialStorage` abstract base class this trait mirrors (and each concrete backend below),
//! and the port plan §3.4 (decisions D-4, D-5, D-13) for the design rationale.
//!
//! # Backends
//!
//! - [`MemoryStore`] — ported from `MemoryStorage` (`auth_storage.py:240-261`): in-process only,
//!   never touches disk or the OS keyring.
//! - [`FileStore`] — ported from `FileStorage` (`auth_storage.py:169-237`): a single JSON file,
//!   written atomically with owner-only (`0600`) permissions.
//! - [`ChainedStore`] — ported from `ChainedStorage` (`auth_storage.py:264-315`): a primary
//!   backend with a fallback, read-through and write-through.
//! - `KeyringStore` (behind `feature = "keyring"`, default-on) — ported from `KeyringStorage`
//!   (`auth_storage.py:121-166`): the OS-native credential store, sharing an entry with `eero-api`
//!   (decision D-5).
//!
//! [`create_storage`] selects among all four with the same four-way matrix as
//! `auth_storage.py:318-344`'s `create_storage()`. Async callers reach any `Arc<dyn
//! CredentialStore>` without blocking their own worker thread via `load_async`, `save_async`,
//! and `clear_async` — thin `tokio::task::spawn_blocking` adapters shared by `transport` (and,
//! in a later round, `auth::mod`'s own inline `spawn_blocking` call sites, which this phase does
//! not touch).

pub mod chained;
pub mod file;
#[cfg(feature = "keyring")]
pub mod keyring;
pub mod memory;

pub use crate::error::StorageError;
pub use chained::ChainedStore;
pub use file::FileStore;
#[cfg(feature = "keyring")]
pub use keyring::KeyringStore;
pub use memory::MemoryStore;

use std::path::PathBuf;
use std::sync::Arc;

use crate::auth::Session;

/// Best-effort re-save-and-read-back migration for a legacy (pre-`schema_version`) record a
/// backend's `load()` just parsed.
///
/// Ported from `_log_migration_readback` (`auth_storage.py:71-92`): a backend with a load path
/// (`FileStore`, `KeyringStore`) that discovers it just parsed a legacy record re-persists the
/// migrated (current-schema) shape and reads it back, logging `debug!` on a match or `warn!` on a
/// mismatch — but **never fails the read**: `session` (the in-memory value already parsed) is
/// always what the caller gets back, regardless of whether the migration write or its read-back
/// succeeded. Neither log line ever includes the credential value itself.
pub(crate) fn migrate_and_verify<S: CredentialStore + ?Sized>(
    store: &S,
    session: &Session,
    backend: &str,
) {
    if store.save(session).is_err() {
        tracing::warn!(
            "Migration read-back did not match for {backend} storage; in-memory credentials \
             are still returned to the caller"
        );
        return;
    }
    match store.load() {
        Ok(reloaded) if reloaded.expose_token() == session.expose_token() => {
            tracing::debug!("Migration read-back verified for {backend} storage");
        }
        _ => {
            tracing::warn!(
                "Migration read-back did not match for {backend} storage; in-memory credentials \
                 are still returned to the caller"
            );
        }
    }
}

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

// ===================== async spawn_blocking adapters =====================

/// Maps a panicked/cancelled blocking task's `JoinError` onto a [`StorageError`].
///
/// [`CredentialStore`]'s methods are synchronous by design (this module's own docs), so
/// [`load_async`]/[`save_async`]/[`clear_async`] run them via `tokio::task::spawn_blocking`; a
/// `JoinError` there means the store implementation itself panicked (or the runtime is shutting
/// down), never that it returned an ordinary `Err`. This is surfaced as a
/// [`StorageError::Backend`] rather than propagating the panic into the caller's own task.
///
/// # Security (phase-2 storage review, finding S4)
///
/// The message deliberately records only [`tokio::task::JoinError::id`],
/// [`tokio::task::JoinError::is_panic`] and [`tokio::task::JoinError::is_cancelled`] — **never**
/// `join_err`'s own `Display`. A previous version of this function interpolated `join_err`
/// directly, on the premise that `JoinError`'s `Display` never carries a credential; that premise
/// is false for the tokio version this crate pins (1.53.1), whose `JoinError::Display` renders a
/// panicking task's payload verbatim (`task {id} panicked with message {panic_str:?}`,
/// `tokio-1.53.1/src/runtime/task/error.rs:139-146`). `CredentialStore` is a public,
/// third-party-implementable trait, so a backend that panics via `unwrap()`/`expect()` on a value
/// carrying the serialized session would put that text straight into this error's `message`,
/// which this crate may log at WARN or return to a caller. Interpolating the `Display` here would
/// reopen exactly the credential-leak path this module otherwise guards against.
fn join_error_to_storage_error(join_err: &tokio::task::JoinError) -> StorageError {
    StorageError::Backend {
        backend: "credential-store-task".to_owned(),
        message: format!(
            "blocking credential-store task {} failed: panicked={}, cancelled={}",
            join_err.id(),
            join_err.is_panic(),
            join_err.is_cancelled()
        ),
    }
}

/// Runs `store.load()` on a blocking thread via `tokio::task::spawn_blocking`, so an async
/// caller (`transport`, and — in a later round — `auth::mod`) never blocks its own worker thread
/// on a synchronous backend call (a native OS keyring API, in particular, always blocks).
///
/// # Errors
///
/// Propagates the store's own [`StorageError`] unchanged. If the blocking task itself panics,
/// the panic is converted to a `StorageError::Backend` instead of taking down the caller's task
/// — see [`join_error_to_storage_error`].
// No call site yet within this crate as of this round: `transport`'s only async store call this
// phase is a save (`Transport::persist_session`, wired to `save_async` below), and `auth::mod`'s
// own inline `spawn_blocking` load call site is explicitly out of scope this round (see this
// module's doc comment). Exercised directly by this module's own tests; the intended landing
// spot is `auth::mod`'s refactor and phase 4's `Client`.
#[allow(dead_code)]
pub(crate) async fn load_async(store: Arc<dyn CredentialStore>) -> Result<Session, StorageError> {
    match tokio::task::spawn_blocking(move || store.load()).await {
        Ok(result) => result,
        Err(join_err) => Err(join_error_to_storage_error(&join_err)),
    }
}

/// Runs `store.save(&session)` on a blocking thread. See [`load_async`]'s docs for the rationale
/// and the panic-handling contract this shares.
///
/// # Errors
///
/// See [`load_async`].
// No call site within this crate's own library code as of this phase: `Transport::set_session`
// persists synchronously from within a caller-provided blocking context (`AuthApi::clear_local_
// and_store`'s own `spawn_blocking`), and `Transport::refresh_session` no longer needs a separate
// async persistence step at all (a successful refresh never rotates the token). Kept `pub(crate)`
// and exercised directly by this module's own tests, since a future async call site (a domain
// module wanting to persist without blocking its own task) is a reasonable addition later.
#[allow(dead_code)]
pub(crate) async fn save_async(
    store: Arc<dyn CredentialStore>,
    session: Session,
) -> Result<(), StorageError> {
    match tokio::task::spawn_blocking(move || store.save(&session)).await {
        Ok(result) => result,
        Err(join_err) => Err(join_error_to_storage_error(&join_err)),
    }
}

/// Runs `store.clear()` on a blocking thread. See [`load_async`]'s docs for the rationale and the
/// panic-handling contract this shares.
///
/// # Errors
///
/// See [`load_async`].
// See `load_async`'s identical justification above: no call site yet within this crate this
// round for the same reason (`auth::mod`'s `clear_local_and_store_warn_only` is the eventual
// caller, but its refactor is out of scope this round). Exercised directly by this module's own
// tests.
#[allow(dead_code)]
pub(crate) async fn clear_async(store: Arc<dyn CredentialStore>) -> Result<(), StorageError> {
    match tokio::task::spawn_blocking(move || store.clear()).await {
        Ok(result) => result,
        Err(join_err) => Err(join_error_to_storage_error(&join_err)),
    }
}

// ===================== create_storage factory =====================

/// Configuration for [`create_storage`]'s backend-selection matrix.
///
/// Mirrors `create_storage()`'s two parameters (`auth_storage.py:318-320`,
/// `use_keyring: bool = True, cookie_file: Optional[str] = None`) as fields rather than function
/// arguments, so a caller can build one with struct-update syntax and so the not-yet-built
/// `Client::builder()` (phase 4) has a single value to thread through. Unlike Python's default
/// (`use_keyring=True`), [`StorageConfig::default`] is the conservative, explicit-opt-in Rust
/// idiom: both fields default to "off" (`Default::default()` derives `false`/`None`, which
/// [`create_storage`] maps to a bare [`MemoryStore`]) — a library should not silently reach for
/// the OS keyring unless a caller asks for it.
#[derive(Debug, Clone, Default)]
pub struct StorageConfig {
    /// Whether to prefer the OS keyring as the primary (or sole) backend.
    ///
    /// Ignored under `--no-default-features` (no `keyring` feature, hence no `KeyringStore`
    /// to build): [`create_storage`] degrades to the next-best backend instead of failing to
    /// compile or panicking — see that function's docs for the exact degradation.
    pub use_keyring: bool,
    /// An optional file path used as the fallback backend (when [`Self::use_keyring`] is also
    /// `true`) or the sole backend (when it is `false`).
    pub cookie_file: Option<PathBuf>,
}

impl StorageConfig {
    /// Equivalent to [`StorageConfig::default`]: both fields off, yielding a bare [`MemoryStore`]
    /// from [`create_storage`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Builds the [`CredentialStore`] backend selected by `config`, mirroring `create_storage()`'s
/// four-way matrix (`auth_storage.py:318-344`), evaluated in the same order:
///
/// | `use_keyring` | `cookie_file` | Result |
/// |---|---|---|
/// | `true` | `Some` | [`ChainedStore`] with `KeyringStore` primary, [`FileStore`] fallback |
/// | `true` | `None` | bare `KeyringStore` |
/// | `false` | `Some` | bare [`FileStore`] |
/// | `false` | `None` | bare [`MemoryStore`] |
///
/// Never fails and never panics — like the Python original, this is a total function over its
/// input.
///
/// # Feature `keyring`
///
/// Under `--no-default-features` there is no `KeyringStore` to build at all, so
/// `config.use_keyring` is ignored and the matrix degrades to whichever non-keyring backend the
/// row would otherwise have included: the `true`+`Some` row becomes a bare `FileStore` (no
/// keyring to chain in front of it) and the `true`+`None` row becomes a bare `MemoryStore` (no
/// keyring, and nothing to fall back to).
#[cfg(feature = "keyring")]
#[must_use]
pub fn create_storage(config: &StorageConfig) -> Arc<dyn CredentialStore> {
    match (config.use_keyring, &config.cookie_file) {
        (true, Some(path)) => Arc::new(ChainedStore::new(
            Arc::new(KeyringStore::new()),
            Arc::new(FileStore::new(path.clone())),
        )),
        (true, None) => Arc::new(KeyringStore::new()),
        (false, Some(path)) => Arc::new(FileStore::new(path.clone())),
        (false, None) => Arc::new(MemoryStore::new()),
    }
}

/// See the `#[cfg(feature = "keyring")]` overload's docs for the full matrix and the
/// degradation this `--no-default-features` build applies.
#[cfg(not(feature = "keyring"))]
#[must_use]
pub fn create_storage(config: &StorageConfig) -> Arc<dyn CredentialStore> {
    match &config.cookie_file {
        Some(path) => Arc::new(FileStore::new(path.clone())),
        None => Arc::new(MemoryStore::new()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        CredentialStore, StorageConfig, StorageError, clear_async, create_storage, load_async,
        save_async,
    };
    use crate::auth::Session;

    // ===================== StorageConfig =====================

    #[test]
    fn storage_config_default_and_new_are_both_off() {
        let default = StorageConfig::default();
        assert!(!default.use_keyring);
        assert!(default.cookie_file.is_none());

        let via_new = StorageConfig::new();
        assert!(!via_new.use_keyring);
        assert!(via_new.cookie_file.is_none());
    }

    // ===================== create_storage matrix — feature = "keyring" =====================
    //
    // The `true`+`None` and `true`+`Some` rows deliberately never call `.save`/`.load`/`.clear`
    // on the resulting store: `KeyringStore::new()` uses the *shared* `eero-api` entry (decision
    // D-5, `SERVICE_NAME`/`ACCOUNT_NAME`), and this container happens to have no reachable
    // keyring backend (see `keyring.rs`'s own tests), but a real backend on a developer's
    // machine would make an actual OS call against a real, shared credential entry. Constructing
    // a `KeyringStore` never touches the OS (only `load`/`save`/`clear` open an `Entry`), so
    // identifying it via `Debug` (which just prints the two field strings, never opens an entry)
    // is representative and safe regardless of the environment this test runs in.
    #[cfg(feature = "keyring")]
    mod keyring_feature_matrix {
        use std::path::PathBuf;

        use super::{Session, StorageConfig, create_storage};

        #[test]
        fn neither_yields_a_working_memory_store() {
            let store = create_storage(&StorageConfig::default());
            assert!(format!("{store:?}").contains("MemoryStore"));
            store
                .save(&Session::from_token("tok"))
                .expect("MemoryStore never fails");
            assert_eq!(
                store.load().expect("load never fails").expose_token(),
                "tok"
            );
        }

        #[test]
        fn file_only_yields_a_working_file_store() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("cookies.json");
            let store = create_storage(&StorageConfig {
                use_keyring: false,
                cookie_file: Some(path.clone()),
            });
            let debug = format!("{store:?}");
            assert!(debug.contains("FileStore"));
            assert!(!debug.contains("ChainedStore"));
            store
                .save(&Session::from_token("tok"))
                .expect("file save succeeds");
            assert!(path.exists());
            assert_eq!(
                store.load().expect("file load succeeds").expose_token(),
                "tok"
            );
        }

        #[test]
        fn keyring_only_yields_a_bare_keyring_store_with_no_file_fallback() {
            let store = create_storage(&StorageConfig {
                use_keyring: true,
                cookie_file: None,
            });
            let debug = format!("{store:?}");
            assert!(debug.contains("KeyringStore"));
            assert!(!debug.contains("ChainedStore"));
            assert!(!debug.contains("FileStore"));
        }

        #[test]
        fn both_yields_a_chained_store_of_keyring_primary_and_file_fallback() {
            let store = create_storage(&StorageConfig {
                use_keyring: true,
                cookie_file: Some(PathBuf::from("/tmp/rusteero-test-does-not-touch-disk.json")),
            });
            let debug = format!("{store:?}");
            assert!(debug.contains("ChainedStore"));
            assert!(debug.contains("KeyringStore"));
            assert!(debug.contains("FileStore"));
        }
    }

    // ===================== create_storage matrix — --no-default-features =====================
    //
    // With no `KeyringStore` to build at all, every row here is a `MemoryStore` or `FileStore` —
    // both perfectly safe to exercise for real (no shared OS state).
    #[cfg(not(feature = "keyring"))]
    mod no_keyring_feature_matrix {
        use super::{Session, StorageConfig, create_storage};

        #[test]
        fn neither_yields_a_working_memory_store() {
            let store = create_storage(&StorageConfig::default());
            assert!(format!("{store:?}").contains("MemoryStore"));
            store
                .save(&Session::from_token("tok"))
                .expect("MemoryStore never fails");
            assert_eq!(
                store.load().expect("load never fails").expose_token(),
                "tok"
            );
        }

        #[test]
        fn file_only_yields_a_working_file_store() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("cookies.json");
            let store = create_storage(&StorageConfig {
                use_keyring: false,
                cookie_file: Some(path.clone()),
            });
            store
                .save(&Session::from_token("tok"))
                .expect("file save succeeds");
            assert!(path.exists());
        }

        #[test]
        fn keyring_requested_with_no_file_degrades_to_a_working_memory_store() {
            let store = create_storage(&StorageConfig {
                use_keyring: true,
                cookie_file: None,
            });
            assert!(format!("{store:?}").contains("MemoryStore"));
            store
                .save(&Session::from_token("tok"))
                .expect("MemoryStore never fails");
            assert_eq!(
                store.load().expect("load never fails").expose_token(),
                "tok"
            );
        }

        #[test]
        fn keyring_requested_with_a_file_degrades_to_a_working_file_store() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("cookies.json");
            let store = create_storage(&StorageConfig {
                use_keyring: true,
                cookie_file: Some(path.clone()),
            });
            assert!(format!("{store:?}").contains("FileStore"));
            store
                .save(&Session::from_token("tok"))
                .expect("file save succeeds");
            assert!(path.exists());
        }
    }

    // ===================== async adapters: happy path =====================

    #[tokio::test]
    async fn save_load_clear_async_round_trip_through_a_shared_store() {
        let store: Arc<dyn CredentialStore> = Arc::new(super::MemoryStore::new());
        save_async(Arc::clone(&store), Session::from_token("tok-async"))
            .await
            .expect("save succeeds");
        let loaded = load_async(Arc::clone(&store)).await.expect("load succeeds");
        assert_eq!(loaded.expose_token(), "tok-async");

        clear_async(Arc::clone(&store))
            .await
            .expect("clear succeeds");
        assert!(!load_async(store).await.expect("load succeeds").is_valid());
    }

    /// A [`CredentialStore`] whose every method returns an ordinary `Err` — isolates an
    /// unexceptional store failure (this test) from a panicking store (the `JoinError` tests
    /// below), which take a different branch inside `load_async`/`save_async`/`clear_async`.
    #[derive(Debug, Default)]
    struct AlwaysFailsStore;

    impl CredentialStore for AlwaysFailsStore {
        fn load(&self) -> Result<Session, StorageError> {
            Err(StorageError::Backend {
                backend: "always-fails".to_owned(),
                message: "deliberate test failure".to_owned(),
            })
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            Err(StorageError::Backend {
                backend: "always-fails".to_owned(),
                message: "deliberate test failure".to_owned(),
            })
        }

        fn clear(&self) -> Result<(), StorageError> {
            Err(StorageError::Backend {
                backend: "always-fails".to_owned(),
                message: "deliberate test failure".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn save_async_propagates_the_stores_own_error_unchanged() {
        let store: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let err = save_async(store, Session::from_token("tok"))
            .await
            .expect_err("the store always fails");
        assert!(matches!(err, StorageError::Backend { backend, .. } if backend == "always-fails"));
    }

    // ===================== async adapters: JoinError path =====================

    /// A [`CredentialStore`] whose every method panics, isolating the `JoinError` branch of
    /// `load_async`/`save_async`/`clear_async` (a panicked blocking task) from an ordinary `Err`
    /// returned by the store itself ([`AlwaysFailsStore`], above).
    #[derive(Debug, Default)]
    struct PanickingStore;

    impl CredentialStore for PanickingStore {
        fn load(&self) -> Result<Session, StorageError> {
            panic!("PanickingStore::load always panics");
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            panic!("PanickingStore::save always panics");
        }

        fn clear(&self) -> Result<(), StorageError> {
            panic!("PanickingStore::clear always panics");
        }
    }

    #[tokio::test]
    async fn load_async_converts_a_panicking_store_into_a_storage_error() {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingStore);
        let err = load_async(store)
            .await
            .expect_err("a panicking store must not take down the caller's task");
        assert!(
            matches!(&err, StorageError::Backend { backend, .. } if backend == "credential-store-task")
        );
        assert!(
            err.to_string().contains("panicked=true"),
            "expected the panic flag recorded in the message, got {err}"
        );
    }

    #[tokio::test]
    async fn save_async_converts_a_panicking_store_into_a_storage_error() {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingStore);
        let err = save_async(store, Session::from_token("tok"))
            .await
            .expect_err("a panicking store must not take down the caller's task");
        assert!(
            matches!(&err, StorageError::Backend { backend, .. } if backend == "credential-store-task")
        );
        assert!(
            err.to_string().contains("panicked=true"),
            "expected the panic flag recorded in the message, got {err}"
        );
    }

    #[tokio::test]
    async fn clear_async_converts_a_panicking_store_into_a_storage_error() {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingStore);
        let err = clear_async(store)
            .await
            .expect_err("a panicking store must not take down the caller's task");
        assert!(
            matches!(&err, StorageError::Backend { backend, .. } if backend == "credential-store-task")
        );
        assert!(
            err.to_string().contains("panicked=true"),
            "expected the panic flag recorded in the message, got {err}"
        );
    }

    // ===================== async adapters: finding S4 =====================

    /// A [`CredentialStore`] whose panic payload is a distinctive, easy-to-grep string, so the
    /// test below can assert it never reaches a [`StorageError`]'s `Display`/`Debug` output.
    #[derive(Debug, Default)]
    struct PanickingWithCredentialLikePayloadStore;

    impl CredentialStore for PanickingWithCredentialLikePayloadStore {
        fn load(&self) -> Result<Session, StorageError> {
            panic!("session_token=leaked-secret-should-never-appear-in-storage-error");
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            panic!("session_token=leaked-secret-should-never-appear-in-storage-error");
        }

        fn clear(&self) -> Result<(), StorageError> {
            panic!("session_token=leaked-secret-should-never-appear-in-storage-error");
        }
    }

    #[tokio::test]
    async fn load_async_never_lets_the_panic_payload_reach_the_storage_error() {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingWithCredentialLikePayloadStore);
        let err = load_async(store)
            .await
            .expect_err("a panicking store must not take down the caller's task");

        let display = err.to_string();
        let debug = format!("{err:?}");
        assert!(
            !display.contains("leaked-secret") && !debug.contains("leaked-secret"),
            "the JoinError's own Display must never be interpolated into a StorageError (S4): \
             display={display:?} debug={debug:?}"
        );
    }
}
