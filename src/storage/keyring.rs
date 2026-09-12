//! OS keyring credential store.
//!
//! Ported from `KeyringStorage` (`eero-api`'s `src/eero/api/auth_storage.py:121-166`). See
//! the const behaviour notes §4 for the full behaviour brief this module
//! implements, and the port plan's decision D-5 for why the service/account pair below must
//! stay byte-for-byte identical to the Python constants.
//!
//! Behind `feature = "keyring"` (default-on): see `src/storage/mod.rs`.

use crate::auth::Session;
use crate::error::StorageError;

use super::CredentialStore;

/// The keyring *service* name, shared byte-for-byte with `eero-api`'s
/// `KeyringStorage.SERVICE_NAME` (`auth_storage.py:124`).
///
/// Deliberately identical across both libraries (decision D-5): a session stored by the Python
/// `eero-api` CLI/library lives in the same OS credential-store entry rusteero reads and
/// writes, so a user who already authenticated with one can use the other without a second
/// one-time-code login. Changing this string silently breaks that interoperability without
/// failing any test that does not explicitly pin the value — see the test below that does.
pub const SERVICE_NAME: &str = "eero-api";

/// The keyring *account* name, shared byte-for-byte with `eero-api`'s
/// `KeyringStorage.ACCOUNT_NAME` (`auth_storage.py:125`).
///
/// See [`SERVICE_NAME`] for why this must never change independently of a coordinated update to
/// both libraries.
pub const ACCOUNT_NAME: &str = "auth-tokens";

/// The backend name reported in every [`StorageError::Backend`] this module produces.
const BACKEND_NAME: &str = "keyring";

/// A [`CredentialStore`] backed by the operating system's native credential store: Keychain
/// Services on macOS, Credential Manager on Windows, or the Secret Service on *nix, via the
/// `keyring` crate's synchronous `Entry` API.
///
/// Ported from `KeyringStorage` (`auth_storage.py:121-166`). [`KeyringStore::new`] uses the
/// exact service/account pair `eero-api` uses ([`SERVICE_NAME`] / [`ACCOUNT_NAME`]), so the two
/// libraries interoperate through one shared entry; [`KeyringStore::with_entry`] is available
/// for callers who deliberately want an isolated entry (tests, or multiple accounts on one
/// machine) — Python's `KeyringStorage` has no equivalent, since its service/account are
/// hardcoded class constants with no override.
///
/// A fresh `keyring::Entry` is opened on every [`CredentialStore::load`]/`save`/`clear` call
/// rather than cached on the struct, so `KeyringStore` itself only ever holds two plain
/// `String`s — no live OS handle, and nothing a `Debug` implementation could leak.
#[derive(Debug, Clone)]
pub struct KeyringStore {
    service: String,
    account: String,
}

impl KeyringStore {
    /// Creates a store using the shared `eero-api` entry ([`SERVICE_NAME`] / [`ACCOUNT_NAME`]).
    ///
    /// This is the constructor that reproduces decision D-5's cross-library interoperability
    /// contract; use [`KeyringStore::with_entry`] only when isolation from that shared entry is
    /// actually wanted.
    #[must_use]
    pub fn new() -> Self {
        Self::with_entry(SERVICE_NAME, ACCOUNT_NAME)
    }

    /// Creates a store using a custom service/account pair.
    ///
    /// Added for the Rust port: `KeyringStorage` hardcodes its service/account as class
    /// constants with no way to override them (`auth_storage.py:124-125`). Prefer
    /// [`KeyringStore::new`] unless a test or a deliberately-isolated deployment needs its own
    /// entry.
    #[must_use]
    pub fn with_entry(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            account: account.into(),
        }
    }

    /// Opens a fresh `keyring::Entry` for this store's service/account pair.
    fn entry(&self) -> Result<keyring::Entry, StorageError> {
        keyring::Entry::new(&self.service, &self.account).map_err(|err| backend_error(&err))
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
    /// Mirrors `KeyringStorage.load()` (`auth_storage.py:127-145`) for the "nothing stored" and
    /// "successfully stored" paths: a missing entry (`keyring::Error::NoEntry`) yields
    /// `Ok(`[`Session::empty`]`())`, and a present entry is parsed through
    /// `Session::from_json`, which already tolerates a malformed or legacy-shaped blob the same
    /// way Python's `AuthCredentials.from_dict()` does.
    ///
    /// # Divergence from eero-api
    ///
    /// `KeyringStorage.load()` wraps its *entire* body — including the backend call itself — in
    /// one bare `except Exception` that logs at DEBUG and falls back to an empty
    /// `AuthCredentials()` (`auth_storage.py:142-145`), so a genuine backend failure (no Secret
    /// Service running, permission denied, etc.) is indistinguishable from "nothing was ever
    /// stored". This is also the root cause of the `ChainedStorage` fallback never triggering in
    /// practice in Python (see the behaviour brief §7): `KeyringStorage.save()` never raises
    /// either, so `ChainedStorage.save()`'s `except` around the primary store can never fire.
    /// rusteero instead returns [`StorageError::Backend`] for a genuine backend failure, so
    /// `ChainedStore`'s primary/fallback logic (built on top of this trait) can actually observe
    /// and react to a real keyring failure instead of silently losing the credential.
    fn load(&self) -> Result<Session, StorageError> {
        let entry = self.entry()?;
        match entry.get_password() {
            Ok(json) => Session::from_json(&json),
            Err(keyring::Error::NoEntry) => Ok(Session::empty()),
            Err(err) => Err(backend_error(&err)),
        }
    }

    /// Serializes `session` via `Session::to_json` and stores it as this entry's password.
    ///
    /// Mirrors `KeyringStorage.save()` (`auth_storage.py:147-155`): one JSON blob per entry, in
    /// `Session::to_json`'s field order (`session_id`, `refresh_token`, `session_expiry`),
    /// exactly reproducing `to_dict()`'s shape.
    ///
    /// # Divergence from eero-api
    ///
    /// `KeyringStorage.save()` swallows every failure (bare `except Exception`, logged at DEBUG,
    /// `auth_storage.py:152-155`) and never raises. rusteero returns [`StorageError::Backend`]
    /// instead, for the same reason documented on [`CredentialStore::load`] above.
    fn save(&self, session: &Session) -> Result<(), StorageError> {
        let entry = self.entry()?;
        let json = session.to_json()?;
        entry.set_password(&json).map_err(|err| backend_error(&err))
    }

    /// Removes this entry from the keyring, if present.
    ///
    /// Mirrors `KeyringStorage.clear()` (`auth_storage.py:157-166`): deleting an entry that was
    /// never stored (`keyring::Error::NoEntry`) is treated as success, matching Python's
    /// specific catch of `PasswordDeleteError` with the comment "Password didn't exist, that's
    /// fine" (`auth_storage.py:162-164`).
    ///
    /// # Divergence from eero-api
    ///
    /// Any *other* failure is returned as [`StorageError::Backend`] rather than swallowed at
    /// DEBUG (`auth_storage.py:165-166`), for the same reason documented on
    /// [`CredentialStore::load`] above.
    fn clear(&self) -> Result<(), StorageError> {
        let entry = self.entry()?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(backend_error(&err)),
        }
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
    use super::{ACCOUNT_NAME, BACKEND_NAME, CredentialStore, KeyringStore, SERVICE_NAME};
    use crate::auth::Session;
    use crate::error::StorageError;

    // ===================== D-5: the shared eero-api entry =====================

    #[test]
    fn service_and_account_match_the_shared_eero_api_entry() {
        // These two strings MUST stay byte-for-byte identical to `auth_storage.py:124-125` —
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
    // Corrected justification (phase-2 storage review, finding S5): the previous comment here
    // claimed "CI runners are headless Linux with no D-Bus session bus at all" as a blanket
    // justification for expecting every one of these tests to fail fast with a typed error.
    // That is false for 2 of the 3 runners in `ci.yml`'s matrix (`ubuntu-latest`, `macos-latest`,
    // `windows-latest`): `keyring` 4.2's default `v1` feature registers
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

    #[test]
    fn load_never_panics_and_never_fabricates_a_valid_session() {
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            "rusteero-test-account-no-backend",
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
            "rusteero-test-account-no-backend",
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
    /// Manager), per finding S5's requirement that a test must never leave a credential behind.
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
            "rusteero-test-account-no-backend",
        );
        // Constructed before the save so it is armed regardless of which branch below runs,
        // and so an assertion failure inside this test still triggers the cleanup.
        let _cleanup = ClearEntryOnDrop(&store);

        let session = Session::from_token("should-never-appear-in-any-error-message");
        match store.save(&session) {
            // Reachable on macOS/Windows CI runners: the write itself is not the bug this
            // finding is about (a genuinely working backend doing genuine work is correct
            // behaviour) — only leaving it behind afterwards would be, and `_cleanup` above
            // removes it unconditionally when this test returns.
            Ok(()) => {}
            Err(StorageError::Backend { backend, message }) => {
                assert_eq!(backend, BACKEND_NAME);
                assert!(!message.contains("should-never-appear-in-any-error-message"));
            }
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
            "rusteero-test-account-no-backend",
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
            "rusteero-test-account-no-backend",
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
