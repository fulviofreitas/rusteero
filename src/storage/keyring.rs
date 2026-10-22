//! OS keyring credential store.
//!
//! Ported from `KeyringStorage` (`eero-api`'s `src/eero/api/auth_storage.py:309-423` at
//! `v8.4.1`). The service/account pair below must stay byte-for-byte identical to the Python
//! constants so the two libraries share one keyring entry.
//!
//! Behind `feature = "keyring"` (default-on): see `src/storage/mod.rs`.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::auth::Session;
use crate::error::StorageError;

use super::CredentialStore;

/// The keyring *service* name, shared byte-for-byte with `eero-api`'s
/// `KeyringStorage.SERVICE_NAME` (`auth_storage.py:314`).
///
/// Deliberately identical across both libraries: a session stored by the Python `eero-api`
/// CLI/library lives in the same OS credential-store entry rusteero reads and writes, so a user
/// who already authenticated with one can use the other without a second one-time-code login.
/// Changing this string silently breaks that interoperability without failing any test that does
/// not explicitly pin the value — see the test below that does.
pub const SERVICE_NAME: &str = "eero-api";

/// The keyring *account* name, shared byte-for-byte with `eero-api`'s
/// `KeyringStorage.ACCOUNT_NAME` (`auth_storage.py:315`).
///
/// See [`SERVICE_NAME`] for why this must never change independently of a coordinated update to
/// both libraries.
pub const ACCOUNT_NAME: &str = "auth-tokens";

/// The backend name reported in every [`StorageError::Backend`] and [`StorageError::NotRetained`]
/// this module produces.
const BACKEND_NAME: &str = "keyring";

/// The fixed `WARN` logged, once, when a save was not retained (and the store is not silenced);
/// never carries the token (`auth_storage.py:402-406`).
const UNPERSISTED_WARNING: &str = "Keyring did not retain the session; credentials are not \
     persisted and will be lost when this process exits";

/// The fixed `WARN` logged, once, when a clear failed (and the store is not silenced); never
/// carries the token. `KeyringStorage._clear_sync` logs this failure at `DEBUG` only and swallows
/// it (`auth_storage.py:421-422`), but this port returns it, and a credential that could not be
/// destroyed is worth a line above `DEBUG`.
const UNCLEARED_WARNING: &str = "Keyring could not be cleared; a stored session may remain in it";

/// A [`CredentialStore`] backed by the operating system's native credential store: Keychain
/// Services on macOS, Credential Manager on Windows, or the Secret Service on *nix, via the
/// `keyring` crate's synchronous `Entry` API.
///
/// Ported from `KeyringStorage` (`auth_storage.py:309-423`). [`KeyringStore::new`] uses the
/// exact service/account pair `eero-api` uses ([`SERVICE_NAME`] / [`ACCOUNT_NAME`]), so the two
/// libraries interoperate through one shared entry; [`KeyringStore::with_entry`] is available
/// for callers who deliberately want an isolated entry (tests, or multiple accounts on one
/// machine) — Python's `KeyringStorage` has no equivalent, since its service/account are
/// hardcoded class constants with no override.
///
/// A fresh `keyring::Entry` is opened on every [`CredentialStore::load`]/`save`/`clear` call
/// rather than cached on the struct, so `KeyringStore` itself only ever holds two plain
/// `String`s, a flag and a lock — no live OS handle, and nothing a `Debug` implementation could
/// leak.
///
/// # Verified saves
///
/// [`CredentialStore::save`] reads the entry back through a fresh `Entry` after `set_password`
/// and returns `Ok(())` only if the stored token is the one saved. A keyring that accepts the
/// write and retains nothing (`keyring.backends.null` in Python) is
/// [`StorageError::NotRetained`]; one that rejects it is [`StorageError::Backend`]. Either way
/// nothing was persisted, so the store logs one fixed-text `WARN` (never the token) unless
/// silenced with [`KeyringStore::warn_on_unpersisted`] — [`create_storage`] silences it when a
/// file fallback sits behind the keyring, where an unretained write is routine and handled. The
/// cause of a rejected write goes to `DEBUG`.
///
/// # Concurrency
///
/// Every operation on a store, and on all of its clones, runs under one internal lock held for
/// the whole operation (the read-back and a legacy-record migration included), so a `save` and a
/// `clear` cannot interleave and call order decides the outcome. Separately constructed stores
/// for the same entry are *not* coordinated with each other, as in `eero-api`.
///
/// [`create_storage`]: super::create_storage
#[derive(Clone)]
pub struct KeyringStore {
    service: String,
    account: String,
    warn_on_unpersisted: bool,
    /// Shared by every clone, which all address the same entry. Taken before the entry is
    /// opened, so it serialises even when no keyring backend exists.
    op_lock: Arc<Mutex<()>>,
}

impl std::fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyringStore")
            .field("service", &self.service)
            .field("account", &self.account)
            .field("warn_on_unpersisted", &self.warn_on_unpersisted)
            .finish_non_exhaustive()
    }
}

impl KeyringStore {
    /// Creates a store using the shared `eero-api` entry ([`SERVICE_NAME`] / [`ACCOUNT_NAME`]).
    ///
    /// This is the constructor that reproduces the cross-library interoperability contract; use
    /// [`KeyringStore::with_entry`] only when isolation from that shared entry is actually
    /// wanted. A save that is not retained logs a `WARN`; see
    /// [`KeyringStore::warn_on_unpersisted`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_entry(SERVICE_NAME, ACCOUNT_NAME)
    }

    /// Creates a store using a custom service/account pair.
    ///
    /// Added for the Rust port: `KeyringStorage` hardcodes its service/account as class
    /// constants with no way to override them (`auth_storage.py:314-315`). Prefer
    /// [`KeyringStore::new`] unless a test or a deliberately-isolated deployment needs its own
    /// entry.
    #[must_use]
    pub fn with_entry(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            account: account.into(),
            warn_on_unpersisted: true,
            op_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Sets whether a save that is not retained, and a clear that fails, log the fixed `WARN`
    /// (default `true`).
    ///
    /// Mirrors `KeyringStorage(warn_on_unpersisted=...)` (`auth_storage.py:319-329`), which
    /// governs the save warning; this port applies it to the clear warning as well, because a
    /// keyring behind a [`super::ChainedStore`] has both reported by the chain. Turn it off when
    /// the keyring sits in front of a file fallback there, where an unretained keyring write is
    /// routine and handled; the cause of a rejected write is still logged at `DEBUG`, and the
    /// failed save or clear is still reported as an error.
    #[must_use]
    pub fn warn_on_unpersisted(mut self, warn: bool) -> Self {
        self.warn_on_unpersisted = warn;
        self
    }

    /// Opens a fresh `keyring::Entry` for this store's service/account pair.
    fn entry(&self) -> Result<keyring::Entry, StorageError> {
        keyring::Entry::new(&self.service, &self.account).map_err(|err| backend_error(&err))
    }

    /// Takes this store's operation lock. A poisoned lock (a panic while it was held) is
    /// recovered: it guards no data, and a later operation must still be able to run.
    fn lock_operations(&self) -> MutexGuard<'_, ()> {
        self.op_lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The body of [`CredentialStore::load`]; the caller holds the operation lock.
    fn load_unlocked(&self) -> Result<Session, StorageError> {
        let entry = self.entry()?;
        match entry.get_password() {
            Ok(json) => {
                let (session, migrated) = Session::from_json_migrating(&json)?;
                if migrated {
                    // Best-effort: never fails this read, see `migrate_and_verify`'s own docs.
                    // The unlocked save keeps the migration inside this operation's lock.
                    super::migrate_and_verify(&session, BACKEND_NAME, |migrated| {
                        self.save_unlocked(migrated)
                    });
                }
                Ok(session)
            }
            Err(keyring::Error::NoEntry) => Ok(Session::empty()),
            Err(err) => Err(backend_error(&err)),
        }
    }

    /// The body of [`CredentialStore::save`]; the caller holds the operation lock. Logs the
    /// outcome (`auth_storage.py:385-407`) and returns it.
    fn save_unlocked(&self, session: &Session) -> Result<(), StorageError> {
        let outcome = self.write_and_read_back(session);
        match &outcome {
            Ok(()) => tracing::debug!("Saved authentication data to keyring"),
            Err(err) => self.report_unpersisted(err),
        }
        outcome
    }

    /// Writes `session` and reads it back through a fresh entry.
    fn write_and_read_back(&self, session: &Session) -> Result<(), StorageError> {
        let entry = self.entry()?;
        let json = session.to_json()?;
        entry
            .set_password(&json)
            .map_err(|err| backend_error(&err))?;
        // A fresh entry, so a backend that caches per `Entry` cannot vouch for itself.
        check_readback(self.entry()?.get_password(), session)
    }

    /// Logs a save that persisted nothing: the cause at `DEBUG` (a read-back that simply found
    /// nothing has no other cause to report) and the one fixed `WARN` unless silenced.
    fn report_unpersisted(&self, err: &StorageError) {
        match err {
            StorageError::NotRetained { .. } => {}
            StorageError::Backend { message, .. } => {
                tracing::debug!("Error saving to keyring: {message}");
            }
            other => tracing::debug!("Error saving to keyring: {other}"),
        }
        if self.warn_on_unpersisted {
            tracing::warn!("{UNPERSISTED_WARNING}");
        }
    }

    /// The body of [`CredentialStore::clear`]; the caller holds the operation lock.
    fn clear_unlocked(&self) -> Result<(), StorageError> {
        let outcome = self.delete_entry();
        if let Err(err) = &outcome {
            match err {
                StorageError::Backend { message, .. } => {
                    tracing::debug!("Error clearing keyring: {message}");
                }
                other => tracing::debug!("Error clearing keyring: {other}"),
            }
            if self.warn_on_unpersisted {
                tracing::warn!("{UNCLEARED_WARNING}");
            }
        }
        outcome
    }

    /// Deletes the entry; an entry that was never there is not an error.
    fn delete_entry(&self) -> Result<(), StorageError> {
        let entry = self.entry()?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(backend_error(&err)),
        }
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for KeyringStore {
    /// Loads the session stored under this entry, or [`Session::empty`] if none exists.
    ///
    /// Mirrors `KeyringStorage.load()` (`auth_storage.py:331-364`) for the "nothing stored" and
    /// "successfully stored" paths: a missing entry (`keyring::Error::NoEntry`) yields
    /// `Ok(`[`Session::empty`]`())`, and a present entry is parsed through
    /// `Session::from_json`, which already tolerates a malformed or legacy-shaped blob the same
    /// way Python's `AuthCredentials.from_dict()` does. A legacy record is migrated in place
    /// inside this same call, under the same lock, and the in-memory session is returned
    /// whether or not the migration write was retained.
    ///
    /// # Divergence from eero-api
    ///
    /// `KeyringStorage.load()` wraps its *entire* body — including the backend call itself — in
    /// one bare `except Exception` that logs at DEBUG and falls back to an empty
    /// `AuthCredentials()` (`auth_storage.py:359-362`), so a genuine backend failure (no Secret
    /// Service running, permission denied, etc.) is indistinguishable from "nothing was ever
    /// stored". rusteero instead returns [`StorageError::Backend`] for a genuine backend
    /// failure, so `ChainedStore`'s primary/fallback logic (built on top of this trait) can
    /// actually observe and react to a real keyring failure instead of silently losing the
    /// credential.
    fn load(&self) -> Result<Session, StorageError> {
        let _guard = self.lock_operations();
        self.load_unlocked()
    }

    /// Serializes `session` via `Session::to_json`, stores it as this entry's password and reads
    /// it back.
    ///
    /// Mirrors `KeyringStorage.save()` (`auth_storage.py:366-407`): one JSON blob per entry, in
    /// `Session::to_json`'s field order (`session_id`, `schema_version`), exactly reproducing
    /// `to_dict()`'s shape. The entry is then read back through a fresh `Entry` and parsed; only
    /// a stored token equal to `session`'s counts as success.
    ///
    /// # Errors
    ///
    /// [`StorageError::NotRetained`] if the keyring accepted the write but the read-back found no
    /// entry, another token, or an unparseable record; [`StorageError::Backend`] if the keyring
    /// rejected the write or the read-back failed for another reason.
    ///
    /// # Divergence from eero-api
    ///
    /// `KeyringStorage.save()` never raises and returns `False` for every failure (the cause
    /// goes to `DEBUG`, `auth_storage.py:385-407`). rusteero returns the failure as a typed
    /// error instead, for the same reason documented on [`CredentialStore::load`] above; the
    /// fixed `WARN` is logged either way.
    fn save(&self, session: &Session) -> Result<(), StorageError> {
        let _guard = self.lock_operations();
        self.save_unlocked(session)
    }

    /// Removes this entry from the keyring, if present.
    ///
    /// Mirrors `KeyringStorage.clear()` (`auth_storage.py:409-422`): deleting an entry that was
    /// never stored (`keyring::Error::NoEntry`) is treated as success, matching Python's
    /// specific catch of `PasswordDeleteError` with the comment "Password didn't exist, that's
    /// fine" (`auth_storage.py:418-420`).
    ///
    /// # Divergence from eero-api
    ///
    /// Any *other* failure is returned as [`StorageError::Backend`] rather than swallowed at
    /// DEBUG (`auth_storage.py:421-422`), for the same reason documented on
    /// [`CredentialStore::load`] above. The cause goes to `DEBUG` as in Python, and the store
    /// also logs one fixed `WARN` (never the token) unless silenced with
    /// [`KeyringStore::warn_on_unpersisted`].
    fn clear(&self) -> Result<(), StorageError> {
        let _guard = self.lock_operations();
        self.clear_unlocked()
    }

    /// Whether the store's own fixed `WARN` is on ([`KeyringStore::warn_on_unpersisted`], on by
    /// default): then every failed save and every failed clear is logged by the store. Silenced,
    /// it logs nothing above `DEBUG` and leaves the report to whoever silenced it, which is a
    /// [`ChainedStore`](super::ChainedStore) in front of a file fallback.
    fn reports_failures(&self) -> bool {
        self.warn_on_unpersisted
    }
}

/// Decides whether a read-back of the entry found the session that was just saved.
///
/// Mirrors the read-back in `KeyringStorage._save_sync` (`auth_storage.py:389-395`): the stored
/// record is parsed like a load would parse it and its token compared with `saved`'s. A missing
/// entry, another token, or a record that does not parse means the keyring accepted the write
/// and retained nothing: [`StorageError::NotRetained`]. Any other read error is the keyring's
/// own failure and stays a [`StorageError::Backend`].
fn check_readback(
    readback: Result<String, keyring::Error>,
    saved: &Session,
) -> Result<(), StorageError> {
    match readback {
        Ok(json) => match Session::from_json(&json) {
            Ok(stored) if stored.expose_token() == saved.expose_token() => Ok(()),
            Ok(_) | Err(_) => Err(not_retained()),
        },
        Err(keyring::Error::NoEntry) => Err(not_retained()),
        Err(err) => Err(backend_error(&err)),
    }
}

/// The error for a write the keyring accepted but did not retain.
fn not_retained() -> StorageError {
    StorageError::NotRetained {
        backend: BACKEND_NAME.to_owned(),
    }
}

/// Wraps a `keyring::Error` as a [`StorageError::Backend`].
///
/// `keyring::Error`'s `Display` (`keyring-core`'s `error.rs`) never embeds a credential value —
/// even its `BadEncoding`/`BadDataFormat` variants, which carry the raw offending bytes, render
/// a fixed description rather than the bytes themselves — so `err.to_string()` is safe to carry
/// into a [`StorageError`] that this crate may eventually log or display.
fn backend_error(err: &keyring::Error) -> StorageError {
    StorageError::Backend {
        backend: BACKEND_NAME.to_owned(),
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::time::Duration;

    use tracing::Level;

    use super::{
        ACCOUNT_NAME, BACKEND_NAME, CredentialStore, KeyringStore, SERVICE_NAME, UNCLEARED_WARNING,
        UNPERSISTED_WARNING, check_readback,
    };
    use crate::auth::Session;
    use crate::error::StorageError;
    use crate::storage::test_support::capture_logs;

    // ===================== the shared eero-api entry =====================

    #[test]
    fn service_and_account_match_the_shared_eero_api_entry() {
        // These two strings MUST stay byte-for-byte identical to `auth_storage.py:314-315` —
        // see this module's doc comment on `SERVICE_NAME` for why a silent drift here would
        // break interoperability without failing any other test.
        assert_eq!(SERVICE_NAME, "eero-api");
        assert_eq!(ACCOUNT_NAME, "auth-tokens");
    }

    #[test]
    fn new_uses_the_shared_service_and_account() {
        let store = KeyringStore::new();
        assert_eq!(store.service, SERVICE_NAME);
        assert_eq!(store.account, ACCOUNT_NAME);
    }

    #[test]
    fn default_is_equivalent_to_new() {
        let store = KeyringStore::default();
        assert_eq!(store.service, SERVICE_NAME);
        assert_eq!(store.account, ACCOUNT_NAME);
    }

    #[test]
    fn with_entry_uses_the_given_service_and_account() {
        let store = KeyringStore::with_entry("custom-service", "custom-account");
        assert_eq!(store.service, "custom-service");
        assert_eq!(store.account, "custom-account");
    }

    #[test]
    fn debug_output_contains_no_credential() {
        // `KeyringStore` only ever holds the service/account identifiers, never a live entry or
        // a token, so there is nothing for a `Debug` impl to redact; this test documents that
        // invariant rather than exercising any redaction logic.
        let store = KeyringStore::with_entry("svc", "acct");
        let debug = format!("{store:?}");
        assert!(debug.contains("svc"));
        assert!(debug.contains("acct"));
    }

    // ===================== behaviour without a *reachable* keyring backend =====================
    //
    // Not every CI runner is a headless Linux box with no D-Bus session bus, so these tests
    // cannot all expect to fail fast with a typed error. Of the three runners in `ci.yml`'s
    // matrix (`ubuntu-latest`, `macos-latest`, `windows-latest`), two have a working native
    // backend: `keyring` 4.2's default `v1` feature registers
    // `apple_native_keyring_store::keychain::Store::new()` on macOS — an **infallible**
    // constructor — and an analogous native, generally-available Windows Credential Manager
    // backend on Windows. On those two platforms `Entry::new()` always succeeds and
    // `get_password`/`set_password`/`delete_credential` talk to a *real*, working credential
    // store; only on Linux (this container, and — as far as this crate can verify — ordinary
    // `ubuntu-latest` runners, which have no desktop session, D-Bus session bus, or Secret
    // Service running) does the backend construction/call itself fail.
    //
    // What this container actually does: `DBUS_SESSION_BUS_ADDRESS=disabled:` — there is no
    // Secret Service reachable at all, so `KeyringStore::entry()` fails before any
    // `get_password`/`set_password`/`delete_credential` call is even attempted, and every method
    // below returns `StorageError::Backend` immediately.
    //
    // What is expected on each `ci.yml` runner:
    //   - `ubuntu-latest`: same as this container — no D-Bus session bus, so `Entry::new()`
    //     fails and every method returns `StorageError::Backend`.
    //   - `macos-latest` / `windows-latest`: a real, working native backend is reachable, so
    //     `load`/`save`/`clear` against a service/account pair that has never been used before
    //     succeed for real, exactly as they would for an end user's own credential.
    //
    // The tests below therefore assert only the property that holds on *every* platform (never
    // panics, and either a clean `StorageError` or an outcome indistinguishable from "nothing
    // stored"), with the stronger, machine-specific assertion gated behind
    // `#[cfg(target_os = "linux")]`. `save` additionally guarantees it never leaves a credential
    // behind on a host that does have a genuinely working backend — see its own test's doc
    // comment for how.

    // Every test below uses its OWN account name. Unit tests run on parallel threads, and on
    // macOS / Windows CI runners the backend is real: a `save` test writing to an entry another
    // test is concurrently `load`ing made `load_never_panics_and_never_fabricates_a_valid_session`
    // observe a live token (CI run 36505627414). Distinct accounts remove the shared state.
    #[test]
    fn load_never_panics_and_never_fabricates_a_valid_session() {
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-load_never_panics_and_never_fabricates_a_valid_session",
        );
        match store.load() {
            // Reachable on macOS/Windows CI runners (infallible native backend, never-used
            // entry) and also possible on Linux if `Entry::new()` succeeds but the entry is
            // simply absent.
            Ok(session) => assert!(!session.is_valid()),
            Err(StorageError::Backend { backend, message }) => {
                assert_eq!(backend, BACKEND_NAME);
                assert!(!message.is_empty());
            }
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    /// Stronger assertion for the one environment this crate is actually developed and CI'd
    /// against with no reachable backend at all: Linux with no D-Bus session bus (see this
    /// section's corrected comment above for why this is Linux-only, not "every CI runner").
    #[cfg(target_os = "linux")]
    #[test]
    fn load_without_a_dbus_session_bus_fails_cleanly_with_no_credential_in_the_message() {
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-load_without_a_dbus_session_bus_fails_cleanly_with_no_credential_in_the_message",
        );
        match store.load() {
            Err(StorageError::Backend { backend, message }) => {
                assert_eq!(backend, BACKEND_NAME);
                assert!(!message.is_empty());
            }
            other => panic!(
                "expected StorageError::Backend on Linux with no D-Bus session bus, got {other:?}"
            ),
        }
    }

    /// Removes whatever [`KeyringStore::with_entry`]'s test-only service/account pair may hold,
    /// on drop — including when a later assertion panics. Exists so tests that call
    /// [`CredentialStore::save`] against this entry can never leave a real credential behind on
    /// a host with a genuinely working native backend (macOS Keychain, Windows Credential
    /// Manager): a test must never leave a credential behind.
    struct ClearEntryOnDrop<'a>(&'a KeyringStore);

    impl Drop for ClearEntryOnDrop<'_> {
        fn drop(&mut self) {
            let _ = self.0.clear();
        }
    }

    #[test]
    fn save_never_leaves_a_credential_behind_on_any_platform() {
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-save_never_leaves_a_credential_behind_on_any_platform",
        );
        // Constructed before the save so it is armed regardless of which branch below runs,
        // and so an assertion failure inside this test still triggers the cleanup.
        let _cleanup = ClearEntryOnDrop(&store);

        let session = Session::from_token("should-never-appear-in-any-error-message");
        match store.save(&session) {
            // Reachable on macOS/Windows CI runners: the write itself is not the problem (a
            // genuinely working backend doing genuine work is correct behaviour) — only leaving
            // it behind afterwards would be, and `_cleanup` above removes it unconditionally
            // when this test returns.
            Ok(()) => {}
            Err(StorageError::Backend { backend, message }) => {
                assert_eq!(backend, BACKEND_NAME);
                assert!(!message.contains("should-never-appear-in-any-error-message"));
            }
            // A backend that accepted the write but did not hand it back.
            Err(StorageError::NotRetained { backend }) => assert_eq!(backend, BACKEND_NAME),
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    /// Stronger assertion for Linux with no D-Bus session bus — see this section's corrected
    /// comment above. Needs no cleanup guard: on this platform the save never succeeds, so there
    /// is never anything to remove.
    #[cfg(target_os = "linux")]
    #[test]
    fn save_without_a_dbus_session_bus_fails_cleanly_with_no_credential_in_the_message() {
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-save_without_a_dbus_session_bus_fails_cleanly_with_no_credential_in_the_message",
        );
        let session = Session::from_token("should-never-appear-in-any-error-message");
        match store.save(&session) {
            Err(StorageError::Backend { backend, message }) => {
                assert_eq!(backend, BACKEND_NAME);
                assert!(!message.contains("should-never-appear-in-any-error-message"));
            }
            other => panic!(
                "expected StorageError::Backend on Linux with no D-Bus session bus, got {other:?}"
            ),
        }
    }

    #[test]
    fn clear_without_a_working_backend_fails_cleanly_or_is_a_no_op() {
        // Unlike `load`/`save`, a missing backend and a missing entry can both plausibly surface
        // here depending on how the platform store reports "not available" vs. "not found"; the
        // one thing that must never happen is a panic, and the one thing a genuine backend
        // failure must never do is print a credential (there is none passed to `clear`, but the
        // message must still be checked for shape). This is also exactly the outcome expected
        // on macOS/Windows CI runners, where the entry genuinely does not exist yet.
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-clear_without_a_working_backend_fails_cleanly_or_is_a_no_op",
        );
        match store.clear() {
            Ok(()) => {}
            Err(StorageError::Backend { backend, message }) => {
                assert_eq!(backend, BACKEND_NAME);
                assert!(!message.is_empty());
            }
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    // ===================== read-back verification (pure) =====================
    //
    // No keyring is involved: `check_readback` judges what a `get_password` returned, so a
    // backend that accepts writes and retains nothing (`keyring.backends.null` in Python) can be
    // described without one.

    #[test]
    fn a_readback_with_no_entry_is_not_retained() {
        let err = check_readback(Err(keyring::Error::NoEntry), &Session::from_token("tok"))
            .expect_err("nothing was stored");
        assert!(matches!(&err, StorageError::NotRetained { backend } if backend == BACKEND_NAME));
    }

    #[test]
    fn a_readback_with_another_token_is_not_retained() {
        let stored = r#"{"session_id":"other","schema_version":2}"#.to_owned();
        let err = check_readback(Ok(stored), &Session::from_token("tok"))
            .expect_err("another token was stored");
        assert!(matches!(err, StorageError::NotRetained { .. }));
    }

    #[test]
    fn a_readback_with_the_same_token_is_retained() {
        let saved = Session::from_token("tok");
        let stored = saved.to_json().expect("serializes");
        check_readback(Ok(stored), &saved).expect("the same token was stored");
    }

    #[test]
    fn a_readback_with_a_legacy_record_of_the_same_token_is_retained() {
        let stored = r#"{"user_token":"tok"}"#.to_owned();
        check_readback(Ok(stored), &Session::from_token("tok"))
            .expect("a legacy record is parsed like a load would parse it");
    }

    #[test]
    fn a_readback_that_does_not_parse_is_not_retained() {
        for garbage in ["not json", "", "[]", "\"tok\"", "null"] {
            let err = check_readback(Ok(garbage.to_owned()), &Session::from_token("tok"))
                .expect_err("not a credential record");
            assert!(
                matches!(err, StorageError::NotRetained { .. }),
                "{garbage:?} -> {err:?}"
            );
        }
    }

    #[test]
    fn any_other_readback_failure_stays_a_backend_error() {
        let failure = keyring::Error::Invalid("service".to_owned(), "too short".to_owned());
        let err = check_readback(Err(failure), &Session::from_token("tok"))
            .expect_err("the keyring itself failed");
        match err {
            StorageError::Backend { backend, message } => {
                assert_eq!(backend, BACKEND_NAME);
                assert!(!message.is_empty());
            }
            other => panic!("expected a backend error, got {other:?}"),
        }
    }

    // ===================== the unpersisted-save warning =====================

    #[test]
    fn new_with_entry_and_default_warn_on_unpersisted() {
        assert!(KeyringStore::new().warn_on_unpersisted);
        assert!(KeyringStore::with_entry("svc", "acct").warn_on_unpersisted);
        assert!(KeyringStore::default().warn_on_unpersisted);
    }

    #[test]
    fn the_builder_turns_the_warning_off_and_back_on() {
        let silenced = KeyringStore::with_entry("svc", "acct").warn_on_unpersisted(false);
        assert!(!silenced.warn_on_unpersisted);
        assert!(
            !silenced.clone().warn_on_unpersisted,
            "a clone keeps the setting"
        );
        assert!(silenced.warn_on_unpersisted(true).warn_on_unpersisted);
    }

    #[test]
    fn debug_shows_the_warning_setting() {
        assert!(format!("{:?}", KeyringStore::new()).contains("warn_on_unpersisted: true"));
        let silenced = KeyringStore::new().warn_on_unpersisted(false);
        assert!(format!("{silenced:?}").contains("warn_on_unpersisted: false"));
    }

    #[test]
    fn a_save_that_was_not_retained_logs_exactly_one_fixed_warning() {
        let (_guard, logs) = capture_logs();
        let store = KeyringStore::with_entry("svc", "acct");

        store.report_unpersisted(&StorageError::NotRetained {
            backend: BACKEND_NAME.to_owned(),
        });

        assert_eq!(
            logs.messages(Level::WARN),
            vec![UNPERSISTED_WARNING.to_owned()]
        );
        assert_eq!(
            UNPERSISTED_WARNING,
            "Keyring did not retain the session; credentials are not persisted and will be lost \
             when this process exits"
        );
        assert!(
            logs.messages(Level::DEBUG).is_empty(),
            "a read-back that found nothing has no cause to report"
        );
    }

    #[test]
    fn a_rejected_save_logs_its_cause_at_debug_and_one_fixed_warning() {
        let (_guard, logs) = capture_logs();
        let store = KeyringStore::with_entry("svc", "acct");

        store.report_unpersisted(&StorageError::Backend {
            backend: BACKEND_NAME.to_owned(),
            message: "no secret service".to_owned(),
        });

        assert_eq!(
            logs.messages(Level::DEBUG),
            vec!["Error saving to keyring: no secret service".to_owned()]
        );
        assert_eq!(
            logs.messages(Level::WARN),
            vec![UNPERSISTED_WARNING.to_owned()]
        );
    }

    #[test]
    fn a_silenced_store_logs_the_cause_at_debug_and_no_warning() {
        let (_guard, logs) = capture_logs();
        let store = KeyringStore::with_entry("svc", "acct").warn_on_unpersisted(false);

        store.report_unpersisted(&StorageError::Backend {
            backend: BACKEND_NAME.to_owned(),
            message: "no secret service".to_owned(),
        });
        store.report_unpersisted(&StorageError::NotRetained {
            backend: BACKEND_NAME.to_owned(),
        });

        assert!(logs.messages(Level::WARN).is_empty());
        assert_eq!(logs.count(Level::DEBUG, "Error saving to keyring"), 1);
    }

    /// Linux with no D-Bus session bus: the save fails before any entry exists, and that
    /// failure must surface as one fixed `WARN` that never carries the token.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_save_without_a_dbus_session_bus_warns_exactly_once_without_the_token() {
        let (_guard, logs) = capture_logs();
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-a_save_without_a_dbus_session_bus_warns_exactly_once_without_the_token",
        );

        let err = store
            .save(&Session::from_token("secret_token"))
            .expect_err("no keyring is reachable");

        assert!(matches!(err, StorageError::Backend { .. }), "got {err:?}");
        assert_eq!(
            logs.count(Level::WARN, "Keyring did not retain the session"),
            1,
            "{:?}",
            logs.messages(Level::WARN)
        );
        assert_eq!(logs.messages(Level::WARN).len(), 1);
        assert_eq!(logs.count(Level::DEBUG, "Error saving to keyring"), 1);
        assert!(!logs.mentions("secret_token"));
    }

    /// Linux with no D-Bus session bus: the clear fails before any entry exists. The cause goes
    /// to `DEBUG` as in Python, and the store adds one fixed `WARN`.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_clear_without_a_dbus_session_bus_warns_exactly_once() {
        let (_guard, logs) = capture_logs();
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-a_clear_without_a_dbus_session_bus_warns_exactly_once",
        );

        let err = store.clear().expect_err("no keyring is reachable");

        assert!(matches!(err, StorageError::Backend { .. }), "got {err:?}");
        assert_eq!(
            logs.messages(Level::WARN),
            vec![UNCLEARED_WARNING.to_owned()]
        );
        assert_eq!(logs.count(Level::DEBUG, "Error clearing keyring"), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_silenced_clear_without_a_dbus_session_bus_logs_only_the_cause() {
        let (_guard, logs) = capture_logs();
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-a_silenced_clear_without_a_dbus_session_bus_logs_only_the_cause",
        )
        .warn_on_unpersisted(false);

        store.clear().expect_err("no keyring is reachable");

        assert!(logs.messages(Level::WARN).is_empty());
        assert_eq!(logs.count(Level::DEBUG, "Error clearing keyring"), 1);
    }

    #[test]
    fn the_keyring_store_reports_its_failures_unless_silenced() {
        assert!(KeyringStore::with_entry("svc", "acct").reports_failures());
        assert!(
            !KeyringStore::with_entry("svc", "acct")
                .warn_on_unpersisted(false)
                .reports_failures()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_silenced_save_without_a_dbus_session_bus_logs_only_the_cause() {
        let (_guard, logs) = capture_logs();
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-a_silenced_save_without_a_dbus_session_bus_logs_only_the_cause",
        )
        .warn_on_unpersisted(false);

        store
            .save(&Session::from_token("secret_token"))
            .expect_err("no keyring is reachable");

        assert!(logs.messages(Level::WARN).is_empty());
        assert_eq!(logs.count(Level::DEBUG, "Error saving to keyring"), 1);
        assert!(!logs.mentions("secret_token"));
    }

    // ===================== per-instance operation lock =====================
    //
    // The lock is taken before the entry is opened, so these work with no keyring backend.

    #[test]
    fn clones_share_one_operation_lock_and_separate_stores_do_not() {
        let store = KeyringStore::with_entry("svc", "acct");
        let clone = store.clone();
        let other = KeyringStore::with_entry("svc", "acct");

        assert!(Arc::ptr_eq(&store.op_lock, &clone.op_lock));
        assert!(!Arc::ptr_eq(&store.op_lock, &other.op_lock));
    }

    /// Runs `operation` on a thread of its own while the test holds the store's lock, and checks
    /// it does not get past the lock until the guard is dropped.
    fn assert_waits_for_the_lock(
        account: &str,
        operation: impl FnOnce(&KeyringStore) + Send + 'static,
    ) {
        let store = KeyringStore::with_entry("rusteero-test-service-no-backend", account);
        // A clone, so the lock is shared the way it is in real use.
        let worker = store.clone();
        let guard = store.lock_operations();

        let (done_tx, done_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            operation(&worker);
            let _ = done_tx.send(());
        });
        assert!(
            done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "the operation ran while the lock was held"
        );

        drop(guard);
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the operation completes once the lock is free");
        thread.join().expect("the operation thread finishes");
    }

    #[test]
    fn load_waits_for_the_operation_lock() {
        assert_waits_for_the_lock("rusteero-test-account-no-backend-op-lock-load", |store| {
            let _ = store.load();
        });
    }

    #[test]
    fn save_waits_for_the_operation_lock() {
        assert_waits_for_the_lock("rusteero-test-account-no-backend-op-lock-save", |store| {
            // On Linux with no backend nothing is ever written; on a host with a real one the
            // entry is removed again right away.
            let _ = store.save(&Session::from_token("should-never-persist"));
            let _ = store.clear();
        });
    }

    #[test]
    fn clear_waits_for_the_operation_lock() {
        assert_waits_for_the_lock("rusteero-test-account-no-backend-op-lock-clear", |store| {
            let _ = store.clear();
        });
    }

    #[test]
    fn a_panic_while_the_lock_is_held_does_not_wedge_the_store() {
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend-poisoned-lock",
        );
        let poisoner = store.clone();
        let result = std::thread::spawn(move || {
            let _guard = poisoner.lock_operations();
            panic!("a store operation panicked");
        })
        .join();
        assert!(result.is_err());

        // A poisoned lock is recovered: the operation proceeds (and fails or succeeds on its
        // merits) instead of panicking.
        let _ = store.clear();
    }

    // ===================== requires a real OS keyring backend =====================

    /// Round-trips a session through a genuine OS keyring backend (Secret Service on Linux,
    /// Keychain on macOS, Credential Manager on Windows).
    ///
    /// Run on a machine that actually has one available, e.g. a Linux desktop session with
    /// `gnome-keyring`/`kwallet` running, or interactively on macOS/Windows:
    ///
    /// ```text
    /// cargo test --all-features -- --ignored keyring_round_trip
    /// ```
    #[test]
    #[ignore = "requires a real OS keyring backend (Secret Service/Keychain/Credential Manager)"]
    fn keyring_round_trip_save_load_clear() {
        let store = KeyringStore::with_entry("rusteero-test-service", "rusteero-test-account");
        let session = Session::from_token("integration-test-token");

        store
            .save(&session)
            .expect("save succeeds against a real backend");
        let loaded = store.load().expect("load succeeds against a real backend");
        assert!(loaded.is_valid());

        store
            .clear()
            .expect("clear succeeds against a real backend");
        let cleared = store.load().expect("load succeeds after clear");
        assert!(!cleared.is_valid());
    }
}
