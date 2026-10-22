//! Pluggable credential storage.
//!
//! Ported from `eero-api`'s `src/eero/api/auth_storage.py` (`v8.4.1`): [`CredentialStore`] mirrors
//! the `CredentialStorage` abstract base class (`auth_storage.py:129-166`) and each backend below
//! mirrors one concrete class.
//!
//! # Backends
//!
//! - [`MemoryStore`] — ported from `MemoryStorage` (`auth_storage.py:609-642`): in-process only,
//!   never touches disk or the OS keyring.
//! - [`FileStore`] — ported from `FileStorage` (`auth_storage.py:425-606`): a single JSON file,
//!   written atomically with owner-only (`0600`) permissions.
//! - [`ChainedStore`] — ported from `ChainedStorage` (`auth_storage.py:645-805`): a primary
//!   backend with a fallback, read-through and write-through.
//! - `KeyringStore` (behind `feature = "keyring"`, default-on) — ported from `KeyringStorage`
//!   (`auth_storage.py:309-423`): the OS-native credential store, sharing an entry with `eero-api`.
//!
//! [`create_storage`] selects among all four with the same four-way matrix as
//! `auth_storage.py:807-830`'s `create_storage()`.
//!
//! # Verified saves
//!
//! A backend can accept a write and retain nothing (a keyring backend that silently drops
//! writes, a file replaced under the writer). So the shipped [`FileStore`], `KeyringStore` and
//! [`ChainedStore`] read the store back after every save, and `save` returns `Ok(())` only if the
//! read-back found the saved token. A write that was accepted but not retained is
//! [`StorageError::NotRetained`]; a write the backend rejected is the backend's own error.
//!
//! # Async callers
//!
//! The trait is synchronous, so an async caller reaches any `Arc<dyn CredentialStore>` without
//! blocking its own worker thread through [`load_async`], [`save_async`] and [`clear_async`]:
//! thin `tokio::task::spawn_blocking` adapters. Each public store operation is one blocking-pool
//! hop, including any read-back or in-place migration it performs. Their docs cover what dropping
//! the returned future does (nothing: the work runs to completion).

pub mod chained;
pub mod file;
#[cfg(feature = "keyring")]
pub mod keyring;
pub mod memory;
#[cfg(test)]
pub(crate) mod test_support;

pub use crate::error::StorageError;
pub use chained::ChainedStore;
pub use file::FileStore;
#[cfg(feature = "keyring")]
pub use keyring::KeyringStore;
pub use memory::MemoryStore;

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::task::{JoinError, JoinHandle};

use crate::auth::Session;

/// Re-saves a legacy (pre-`schema_version`) record a backend's `load()` just parsed, in the
/// current schema, and logs whether the store retained it.
///
/// Ported from `_log_migration_readback` (`auth_storage.py:84-104`) and its two callers
/// (`KeyringStorage._load_sync` `:346-364`, `FileStorage._load_sync` `:461-494`). `save` is the
/// backend's own *unlocked* save, which already reads the store back (see the module docs), so
/// its `Ok` is the verified migration: this logs `debug!` on `Ok` and `warn!` otherwise. It runs
/// inside the load that found the legacy record, on the same thread and under the same operation
/// lock, so a load is one unit of work. The read-back is the one inside `save`, never a call back
/// into `load`, so a backend that accepts the write but keeps serving the legacy record cannot
/// recurse into another migration.
///
/// It **never fails the read**: `session` (the in-memory value already parsed) is always what the
/// caller gets back, whether or not the migration write was retained. Neither log line ever
/// includes the credential value itself.
pub(crate) fn migrate_and_verify(
    session: &Session,
    backend: &str,
    save: impl FnOnce(&Session) -> Result<(), StorageError>,
) {
    tracing::debug!("Migrated legacy credential record in {backend} storage");
    if save(session).is_ok() {
        tracing::debug!("Migration read-back verified for {backend} storage");
    } else {
        tracing::warn!(
            "Migration read-back did not match for {backend} storage; in-memory credentials \
             are still returned to the caller"
        );
    }
}

/// A pluggable backend for persisting a [`Session`] across process restarts.
///
/// Mirrors the `CredentialStorage` abstract base class (`auth_storage.py:129-166`), which
/// declares three `async` abstract methods (`load`, `save`, `clear`) with no default bodies.
///
/// This trait is deliberately **synchronous**, not `async`, even though every method it
/// mirrors is `async def` in Python: every native OS keyring API (`keyring` crate,
/// `feature = "keyring"`) is a blocking call, so an async trait here would only move the
/// blocking work around, not remove it, while forcing `async-trait` boxing on every
/// implementation for no benefit. Async callers invoke these methods through [`load_async`],
/// [`save_async`] and [`clear_async`] (or their own `tokio::task::spawn_blocking`).
///
/// Implementations must be `Send + Sync` (usable from any task/thread) and `Debug` (so a
/// `Client` holding one behind an `Arc<dyn CredentialStore>` can still derive/implement
/// `Debug` itself).
///
/// # Concurrency
///
/// The shipped [`FileStore`] and `KeyringStore` serialise their own `load`/`save`/`clear` with an
/// internal lock that every clone of the store shares, so a `save` and a `clear` on one instance
/// cannot interleave and call order decides the outcome (`auth_storage.py:190-235`). Separate
/// instances addressing the same file or keyring entry are *not* coordinated with each other.
/// [`MemoryStore`] is lock-guarded, and [`ChainedStore`] inherits its backends' serialisation (it
/// does not make an operation atomic across the two). Third-party implementations that touch
/// shared external state should serialise their own operations the same way, because async
/// callers can have several in flight on different blocking-pool threads.
pub trait CredentialStore: Send + Sync + std::fmt::Debug {
    /// Loads the stored session.
    ///
    /// Returns `Ok(`[`Session::empty`]`())`, **not** an error, when nothing has ever been
    /// stored — mirroring every Python backend's "never seen before" behaviour (e.g.
    /// `MemoryStorage.__init__`'s fresh `AuthCredentials()`, `auth_storage.py:616-618`, or
    /// `FileStorage.load()`'s "file not found" branch, `auth_storage.py:463-466`).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] only when the backend itself is unusable (e.g. an I/O failure
    /// reading a file, or a keyring backend reporting an error) — never merely because nothing
    /// has been saved yet. A `Client` builder warns on such a failure by default and treats it
    /// as fatal only on request.
    fn load(&self) -> Result<Session, StorageError>;

    /// Persists `session`, overwriting whatever was previously stored.
    ///
    /// The shipped backends return `Ok(())` only after a read-back found the saved token (see
    /// the module docs), like `CredentialStorage.save()` returning `True` only when a read-back
    /// found the saved `session_id` (`auth_storage.py:142-161`). A [`MemoryStore`] record counts
    /// as retained even though it does not outlive the process. A third-party implementation is
    /// trusted: `Ok(())` from it is taken to mean the session was persisted.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the backend could not persist the session (e.g. a file
    /// system permission failure), or [`StorageError::NotRetained`] if the backend accepted the
    /// write but a read-back did not find the saved session. Unlike every Python backend (which
    /// never raises and returns `False`, logging the cause at DEBUG/ERROR,
    /// `auth_storage.py:385-407,541-593`), this port surfaces the failure as a typed error so a
    /// caller can decide whether it is fatal.
    fn save(&self, session: &Session) -> Result<(), StorageError>;

    /// Removes the stored session entirely.
    ///
    /// Mirrors `clear()` (e.g. `auth_storage.py:409-423,595-606,640-642`): unlike
    /// [`CredentialStore::save`] with an [`Session::empty`], this removes the underlying
    /// storage entry/file rather than overwriting it with null fields.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the backend failed to remove the stored entry. Removing an
    /// entry that was never there is *not* an error (mirrors every Python backend's tolerant
    /// `clear()`, e.g. the swallowed `PasswordDeleteError` at `auth_storage.py:418-420`).
    fn clear(&self) -> Result<(), StorageError>;
}

// ===================== async spawn_blocking adapters =====================

/// The backend name of every [`StorageError::Backend`] the async adapters raise on behalf of the
/// blocking task itself (a panic, a cancellation, a task that could not start), as opposed to an
/// error the store returned.
const TASK_BACKEND: &str = "credential-store-task";

/// Maps a panicked/cancelled blocking task's `JoinError` onto a [`StorageError`].
///
/// [`CredentialStore`]'s methods are synchronous by design (this module's own docs), so
/// [`load_async`]/[`save_async`]/[`clear_async`] run them via `tokio::task::spawn_blocking`; a
/// `JoinError` there means the store implementation itself panicked (or the runtime is shutting
/// down), never that it returned an ordinary `Err`. This is surfaced as a
/// [`StorageError::Backend`] rather than propagating the panic into the caller's own task.
///
/// # Security
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
fn join_error_to_storage_error(join_err: &JoinError) -> StorageError {
    StorageError::Backend {
        backend: TASK_BACKEND.to_owned(),
        message: format!(
            "blocking credential-store task {} failed: panicked={}, cancelled={}",
            join_err.id(),
            join_err.is_panic(),
            join_err.is_cancelled()
        ),
    }
}

/// Runs `spawn`, turning a panic out of it into a [`StorageError`] instead of unwinding into the
/// caller.
///
/// `tokio::task::spawn_blocking` panics when it cannot start the task: the OS refused to create a
/// worker thread and no blocking-pool thread is alive to take the work (`OS can't spawn worker
/// thread: ..`, `tokio-1.53.1/src/runtime/blocking/pool.rs:324-326`), or it is called outside a
/// runtime. A save that cannot start must stay a handled storage failure (`save()` never raises
/// in `eero-api`, `auth_storage.py:262-280`), so the panic is caught here. The message is fixed
/// and never interpolates the panic payload. The default panic hook still prints the original
/// panic message, and nothing can be caught under `panic = "abort"`.
///
/// Split from [`spawn_store_task`] so a test can inject a spawner that panics.
fn spawn_guarded<R>(spawn: impl FnOnce() -> JoinHandle<R>) -> Result<JoinHandle<R>, StorageError> {
    std::panic::catch_unwind(AssertUnwindSafe(spawn)).map_err(|_payload| StorageError::Backend {
        backend: TASK_BACKEND.to_owned(),
        message: "blocking credential-store task could not start".to_owned(),
    })
}

/// Hands `work` to the blocking pool, or reports why it could not be started.
///
/// The caller's `tracing` dispatcher and current span travel with the work. A blocking-pool
/// thread knows nothing of a subscriber installed with `tracing::subscriber::set_default` or
/// `with_default` on the calling thread, so without this every line a store logs (the fixed
/// `WARN`s of the keyring and file stores, say) would go to the global subscriber or nowhere,
/// instead of to whoever is observing the task that asked for the store call.
fn spawn_store_task<R: Send + 'static>(
    work: impl FnOnce() -> R + Send + 'static,
) -> Result<JoinHandle<R>, StorageError> {
    let dispatch = tracing::dispatcher::get_default(Clone::clone);
    let span = tracing::Span::current();
    spawn_guarded(|| {
        tokio::task::spawn_blocking(move || {
            tracing::dispatcher::with_default(&dispatch, || span.in_scope(work))
        })
    })
}

/// Flattens a joined blocking task's outcome into the store's own result, mapping a `JoinError`
/// through [`join_error_to_storage_error`].
fn flatten_join<T>(joined: Result<Result<T, StorageError>, JoinError>) -> Result<T, StorageError> {
    match joined {
        Ok(result) => result,
        Err(join_err) => Err(join_error_to_storage_error(&join_err)),
    }
}

/// Runs `store.load()` on a blocking thread via `tokio::task::spawn_blocking`, so an async
/// caller never blocks its own worker thread on a synchronous backend call (a native OS keyring
/// API, in particular, always blocks).
///
/// The whole operation is one hop: a load that finds a legacy record also performs the migration
/// write and its read-back on that same thread, inside the store's own operation lock
/// (`auth_storage.py:346-364`). Needs a running Tokio runtime; it works on both the current-thread
/// and the multi-thread flavours, and a store can be reused across successive runtimes.
///
/// # Errors
///
/// Propagates the store's own [`StorageError`] unchanged. A failure of the blocking task itself is
/// a [`StorageError::Backend`] with the backend name `"credential-store-task"` and a fixed
/// message that never carries a panic payload: the store panicked, the runtime was shutting down,
/// or the task could not be started (the OS refused a worker thread). The same holds for
/// [`save_async`] and [`clear_async`].
///
/// # Cancellation
///
/// The store call is handed to the blocking pool in the first poll of the returned future,
/// before it first yields. Dropping the future (or aborting the task that awaits it) after that
/// does not cancel anything: a running blocking task cannot be interrupted, and one still waiting
/// for the store's operation lock runs as soon as the lock frees. So a `save` always finishes its
/// atomic write, leaving the old or the new record and never a partial one, and a later
/// operation on the same store instance waits for it instead of racing it. The cancelled caller
/// never sees the result. This differs from `eero-api`, where cancelling an operation still
/// waiting for the lock skips it (`auth_storage.py:205-213`): here dropping a future never skips
/// a store write or clear, so the store cannot end up disagreeing with an in-memory session that
/// was already changed. Dropping the future before its first poll never starts the task at all.
///
/// # Blocking threads
///
/// Each call occupies one thread of the runtime's blocking pool for as long as the store call
/// takes. A store that serialises its operations behind a lock (the shipped file and keyring
/// stores do) holds that thread while it waits.
///
/// # Logging
///
/// What the store logs on the blocking thread is recorded under the `tracing` dispatcher and
/// span that were current when the future was first polled, so a subscriber scoped to the
/// calling task sees it. The same holds for [`save_async`] and [`clear_async`].
pub async fn load_async(store: Arc<dyn CredentialStore>) -> Result<Session, StorageError> {
    flatten_join(spawn_store_task(move || store.load())?.await)
}

/// Runs `store.save(&session)` on a blocking thread, with the read-back and everything else the
/// store does on save in the same hop. See [`load_async`] for the cancellation, blocking-thread
/// and failure contract this shares.
///
/// # Errors
///
/// See [`load_async`]. A store that accepted the write but did not retain it reports
/// [`StorageError::NotRetained`].
pub async fn save_async(
    store: Arc<dyn CredentialStore>,
    session: Session,
) -> Result<(), StorageError> {
    flatten_join(spawn_store_task(move || store.save(&session))?.await)
}

/// Runs `store.clear()` on a blocking thread. See [`load_async`] for the cancellation,
/// blocking-thread and failure contract this shares.
///
/// # Errors
///
/// See [`load_async`].
pub async fn clear_async(store: Arc<dyn CredentialStore>) -> Result<(), StorageError> {
    flatten_join(spawn_store_task(move || store.clear())?.await)
}

// ===================== create_storage factory =====================

/// Configuration for [`create_storage`]'s backend-selection matrix.
///
/// Mirrors `create_storage()`'s two parameters (`auth_storage.py:807-810`,
/// `use_keyring: bool = True, cookie_file: Optional[str] = None`) as fields rather than function
/// arguments, so a caller can build one with struct-update syntax and thread a single value
/// through. Unlike Python's default
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
/// four-way matrix (`auth_storage.py:807-830`), evaluated in the same order:
///
/// | `use_keyring` | `cookie_file` | Result |
/// |---|---|---|
/// | `true` | `Some` | [`ChainedStore`] with `KeyringStore` primary, [`FileStore`] fallback |
/// | `true` | `None` | bare `KeyringStore` |
/// | `false` | `Some` | bare [`FileStore`] |
/// | `false` | `None` | bare [`MemoryStore`] |
///
/// In the chained row the keyring is built with `warn_on_unpersisted(false)`: a keyring that
/// retains nothing is routine and handled there (the chain falls back to the file), so only the
/// bare-keyring row warns when a save is not retained (`auth_storage.py:823`).
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
            Arc::new(KeyringStore::new().warn_on_unpersisted(false)),
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
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, PoisonError, mpsc};
    use std::thread::ThreadId;
    use std::time::{Duration, Instant};

    use tracing::Level;

    use super::test_support::capture_logs;
    use super::{
        CredentialStore, FileStore, StorageConfig, StorageError, clear_async, create_storage,
        load_async, migrate_and_verify, save_async, spawn_guarded,
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

    // ===================== migration: one unit, no recursion =====================

    /// A store holding a legacy blob in memory that records the thread of every call, to prove a
    /// migrating load stays on the one blocking thread the adapter picked.
    #[derive(Debug)]
    struct ThreadRecordingStore {
        blob: Mutex<String>,
        threads: Mutex<Vec<ThreadId>>,
    }

    impl ThreadRecordingStore {
        fn record(&self) {
            self.threads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(std::thread::current().id());
        }
    }

    impl CredentialStore for ThreadRecordingStore {
        fn load(&self) -> Result<Session, StorageError> {
            self.record();
            let blob = self
                .blob
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let (session, migrated) = Session::from_json_migrating(&blob)?;
            if migrated {
                migrate_and_verify(&session, "recording", |migrated| self.save(migrated));
            }
            Ok(session)
        }

        fn save(&self, session: &Session) -> Result<(), StorageError> {
            self.record();
            *self.blob.lock().unwrap_or_else(PoisonError::into_inner) = session.to_json()?;
            Ok(())
        }

        fn clear(&self) -> Result<(), StorageError> {
            self.record();
            Ok(())
        }
    }

    #[tokio::test]
    async fn load_async_runs_a_migrating_load_on_one_blocking_thread() {
        let store = Arc::new(ThreadRecordingStore {
            blob: Mutex::new(r#"{"user_token":"tok"}"#.to_owned()),
            threads: Mutex::new(Vec::new()),
        });

        let session = load_async(Arc::clone(&store) as _)
            .await
            .expect("load succeeds");

        assert_eq!(session.expose_token(), "tok");
        let stored = store.blob.lock().expect("blob lock").clone();
        assert!(
            stored.contains(r#""schema_version":2"#),
            "the legacy record was migrated in place: {stored}"
        );
        let mut threads = store.threads.lock().expect("threads lock").clone();
        assert!(
            threads.len() >= 2,
            "the load and the migration write both ran"
        );
        threads.sort_by_key(|id| format!("{id:?}"));
        threads.dedup();
        assert_eq!(threads.len(), 1, "one thread hop for the whole load");
        assert_ne!(threads[0], std::thread::current().id());
    }

    #[tokio::test]
    async fn load_async_migrates_a_legacy_file_record_in_one_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        std::fs::write(&path, r#"{"user_token":"tok"}"#).expect("seed the legacy record");
        let store: Arc<dyn CredentialStore> = Arc::new(FileStore::new(&path));

        let session = load_async(Arc::clone(&store)).await.expect("load succeeds");

        assert_eq!(session.expose_token(), "tok");
        let on_disk = std::fs::read_to_string(&path).expect("the file still exists");
        assert!(on_disk.contains(r#""schema_version":2"#), "{on_disk}");
    }

    /// A store that accepts every write and keeps serving the legacy record it started with.
    #[derive(Debug, Default)]
    struct StubbornStore {
        loads: AtomicUsize,
        saves: AtomicUsize,
    }

    impl CredentialStore for StubbornStore {
        fn load(&self) -> Result<Session, StorageError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            let (session, migrated) = Session::from_json_migrating(r#"{"user_token":"tok"}"#)?;
            if migrated {
                migrate_and_verify(&session, "stubborn", |migrated| self.save(migrated));
            }
            Ok(session)
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn clear(&self) -> Result<(), StorageError> {
            Ok(())
        }
    }

    #[test]
    fn a_store_that_keeps_serving_the_legacy_record_does_not_recurse() {
        let store = StubbornStore::default();

        let session = store.load().expect("load succeeds");

        assert_eq!(session.expose_token(), "tok");
        assert_eq!(store.loads.load(Ordering::SeqCst), 1, "no re-entrant load");
        assert_eq!(store.saves.load(Ordering::SeqCst), 1, "one migration write");
    }

    #[test]
    fn migrate_and_verify_logs_the_outcome_of_the_save_and_never_the_token() {
        let (_guard, logs) = capture_logs();
        let session = Session::from_token("legacy_token");

        migrate_and_verify(&session, "keyring", |_| Ok(()));
        assert_eq!(
            logs.count(
                Level::DEBUG,
                "Migrated legacy credential record in keyring storage"
            ),
            1
        );
        assert_eq!(
            logs.count(
                Level::DEBUG,
                "Migration read-back verified for keyring storage"
            ),
            1
        );
        assert!(logs.messages(Level::WARN).is_empty());

        migrate_and_verify(&session, "file", |_| {
            Err(StorageError::NotRetained {
                backend: "file".to_owned(),
            })
        });
        assert_eq!(
            logs.messages(Level::WARN),
            vec![
                "Migration read-back did not match for file storage; in-memory credentials are \
                 still returned to the caller"
                    .to_owned()
            ]
        );
        assert!(!logs.mentions("legacy_token"));
    }

    // ===================== async adapters: cancellation, responsiveness, spawn failure =====================

    /// A store over a [`FileStore`] whose `save` and `clear` park, on the blocking-pool thread that
    /// runs them, until the test releases them, and report when they finish.
    #[derive(Debug)]
    struct GatedStore {
        inner: FileStore,
        entered: Mutex<mpsc::Sender<ThreadId>>,
        release: Mutex<mpsc::Receiver<()>>,
        finished: Mutex<mpsc::Sender<()>>,
    }

    /// The test's side of a [`GatedStore`].
    struct Gate {
        entered: Arc<Mutex<mpsc::Receiver<ThreadId>>>,
        release: mpsc::Sender<()>,
        finished: Arc<Mutex<mpsc::Receiver<()>>>,
    }

    fn gated(inner: FileStore) -> (Arc<GatedStore>, Gate) {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let store = Arc::new(GatedStore {
            inner,
            entered: Mutex::new(entered_tx),
            release: Mutex::new(release_rx),
            finished: Mutex::new(finished_tx),
        });
        let gate = Gate {
            entered: Arc::new(Mutex::new(entered_rx)),
            release: release_tx,
            finished: Arc::new(Mutex::new(finished_rx)),
        };
        (store, gate)
    }

    impl GatedStore {
        fn park(&self) {
            let _ = self
                .entered
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(std::thread::current().id());
            let _ = self
                .release
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .recv_timeout(Duration::from_secs(5));
        }

        fn finish(&self) {
            let _ = self
                .finished
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(());
        }
    }

    impl CredentialStore for GatedStore {
        fn load(&self) -> Result<Session, StorageError> {
            self.inner.load()
        }

        fn save(&self, session: &Session) -> Result<(), StorageError> {
            self.park();
            let result = self.inner.save(session);
            self.finish();
            result
        }

        fn clear(&self) -> Result<(), StorageError> {
            self.park();
            let result = self.inner.clear();
            self.finish();
            result
        }
    }

    /// Waits for the next signal without blocking the test's runtime.
    async fn next<T: Send + 'static>(receiver: &Arc<Mutex<mpsc::Receiver<T>>>) -> T {
        let receiver = Arc::clone(receiver);
        tokio::task::spawn_blocking(move || {
            receiver
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .recv_timeout(Duration::from_secs(5))
                .expect("the gated store signals in time")
        })
        .await
        .expect("the waiting task finishes")
    }

    /// Reads the token straight from disk.
    fn token_on_disk(path: &Path) -> Option<String> {
        let contents = std::fs::read_to_string(path).ok()?;
        Some(
            Session::from_json(&contents)
                .ok()?
                .expose_token()
                .to_owned(),
        )
    }

    #[tokio::test]
    async fn dropping_save_async_does_not_interrupt_the_blocking_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        FileStore::new(&path)
            .save(&Session::from_token("old"))
            .expect("seed save succeeds");
        let (store, gate) = gated(FileStore::new(&path));

        let task = tokio::spawn(save_async(store, Session::from_token("new")));
        next(&gate.entered).await;
        task.abort();
        assert!(
            task.await
                .expect_err("the awaiting task was aborted")
                .is_cancelled()
        );
        assert_eq!(
            token_on_disk(&path).as_deref(),
            Some("old"),
            "the parked write has not happened yet, and nothing partial is on disk"
        );

        // The worker thread keeps going: the cancelled caller just never sees the result.
        gate.release.send(()).expect("the worker is parked");
        next(&gate.finished).await;
        let reloaded = load_async(Arc::new(FileStore::new(&path)))
            .await
            .expect("load succeeds");
        assert_eq!(reloaded.expose_token(), "new");

        let listing: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(
            listing,
            vec![std::ffi::OsString::from("cookies.json")],
            "no temporary file is left behind"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("file exists")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    /// Runs `ticks` short sleeps on the test's current-thread runtime and returns how long that
    /// took: a runtime blocked by a store call could not complete them.
    async fn heartbeat(ticks: u32) -> Duration {
        let started = Instant::now();
        tokio::time::timeout(Duration::from_secs(5), async {
            for _ in 0..ticks {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the runtime keeps ticking while the store call is parked");
        started.elapsed()
    }

    #[tokio::test]
    async fn save_async_keeps_the_runtime_responsive_while_the_write_blocks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let (store, gate) = gated(FileStore::new(&path));

        let save = tokio::spawn(save_async(store, Session::from_token("tok")));
        let worker = next(&gate.entered).await;
        assert_ne!(
            worker,
            std::thread::current().id(),
            "the write runs off-runtime"
        );

        assert!(heartbeat(10).await < Duration::from_secs(2));
        assert!(!save.is_finished(), "the write is still parked");

        gate.release.send(()).expect("the worker is parked");
        save.await
            .expect("the save task finishes")
            .expect("save succeeds");
        assert_eq!(token_on_disk(&path).as_deref(), Some("tok"));
    }

    #[tokio::test]
    async fn clear_async_keeps_the_runtime_responsive_while_the_clear_blocks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        FileStore::new(&path)
            .save(&Session::from_token("tok"))
            .expect("seed save succeeds");
        let (store, gate) = gated(FileStore::new(&path));

        let clear = tokio::spawn(clear_async(store));
        let worker = next(&gate.entered).await;
        assert_ne!(
            worker,
            std::thread::current().id(),
            "the clear runs off-runtime"
        );

        assert!(heartbeat(10).await < Duration::from_secs(2));
        assert!(!clear.is_finished(), "the clear is still parked");

        gate.release.send(()).expect("the worker is parked");
        clear
            .await
            .expect("the clear task finishes")
            .expect("clear succeeds");
        assert!(!path.exists());
    }

    #[test]
    fn a_file_store_works_across_two_successive_runtimes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store: Arc<dyn CredentialStore> =
            Arc::new(FileStore::new(dir.path().join("cookies.json")));

        for token in ["a", "b"] {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a current-thread runtime builds");
            runtime.block_on(async {
                save_async(Arc::clone(&store), Session::from_token(token))
                    .await
                    .expect("save succeeds");
                let loaded = load_async(Arc::clone(&store)).await.expect("load succeeds");
                assert_eq!(loaded.expose_token(), token);
            });
        }
    }

    #[tokio::test]
    async fn a_worker_thread_that_cannot_be_spawned_is_a_handled_storage_error() {
        let err = spawn_guarded::<()>(|| panic!("OS can't spawn worker thread: test"))
            .expect_err("the spawner panicked");

        match &err {
            StorageError::Backend { backend, message } => {
                assert_eq!(backend, "credential-store-task");
                assert_eq!(message, "blocking credential-store task could not start");
            }
            other => panic!("expected a backend error, got {other:?}"),
        }
        assert!(
            !format!("{err} {err:?}").contains("OS can't spawn"),
            "the panic payload never reaches the error"
        );

        // Nothing was left wedged: the next real operation works.
        let store: Arc<dyn CredentialStore> = Arc::new(super::MemoryStore::new());
        save_async(Arc::clone(&store), Session::from_token("tok"))
            .await
            .expect("a following save succeeds");
        assert_eq!(
            load_async(store)
                .await
                .expect("load succeeds")
                .expose_token(),
            "tok"
        );
    }

    /// A store that accepts every write and retains nothing, like a keyring that drops writes.
    #[derive(Debug)]
    struct NullStore;

    impl CredentialStore for NullStore {
        fn load(&self) -> Result<Session, StorageError> {
            Ok(Session::empty())
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            Err(StorageError::NotRetained {
                backend: "null".to_owned(),
            })
        }

        fn clear(&self) -> Result<(), StorageError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn save_async_returns_a_not_retained_error_unchanged() {
        let err = save_async(Arc::new(NullStore), Session::from_token("tok"))
            .await
            .expect_err("the store did not retain the session");

        assert!(matches!(&err, StorageError::NotRetained { backend } if backend == "null"));
    }

    // ===================== async adapters: the caller's subscriber follows the work =====================

    /// A store that logs a `WARN` from inside every operation and records the span that was
    /// current while it ran.
    #[derive(Debug, Default)]
    struct WarningStore {
        spans: Mutex<Vec<Option<tracing::Id>>>,
    }

    impl WarningStore {
        fn note(&self) {
            tracing::warn!("store warning from the blocking thread");
            self.spans
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(tracing::Span::current().id());
        }
    }

    impl CredentialStore for WarningStore {
        fn load(&self) -> Result<Session, StorageError> {
            self.note();
            Ok(Session::empty())
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            self.note();
            Ok(())
        }

        fn clear(&self) -> Result<(), StorageError> {
            self.note();
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_store_warning_inside_save_async_reaches_the_callers_scoped_subscriber() {
        let (_guard, logs) = capture_logs();
        let store = Arc::new(WarningStore::default());

        save_async(Arc::clone(&store) as _, Session::from_token("tok"))
            .await
            .expect("save succeeds");

        assert_eq!(
            logs.count(Level::WARN, "store warning from the blocking thread"),
            1,
            "the blocking thread logs under the dispatcher that was current when it was spawned"
        );
    }

    #[tokio::test]
    async fn load_and_clear_async_forward_the_callers_subscriber_too() {
        let (_guard, logs) = capture_logs();
        let store = Arc::new(WarningStore::default());

        load_async(Arc::clone(&store) as _)
            .await
            .expect("load succeeds");
        clear_async(Arc::clone(&store) as _)
            .await
            .expect("clear succeeds");

        assert_eq!(
            logs.count(Level::WARN, "store warning from the blocking thread"),
            2
        );
    }

    #[tokio::test]
    async fn the_callers_current_span_is_current_inside_the_store_call() {
        use tracing::Instrument;

        let (_guard, _logs) = capture_logs();
        let store = Arc::new(WarningStore::default());
        let span = tracing::info_span!("caller");
        let expected = span.id();
        assert!(
            expected.is_some(),
            "the capturing subscriber assigns span ids"
        );

        save_async(Arc::clone(&store) as _, Session::from_token("tok"))
            .instrument(span)
            .await
            .expect("save succeeds");

        let seen = store.spans.lock().expect("spans lock").clone();
        assert_eq!(seen, vec![expected]);
    }
}
