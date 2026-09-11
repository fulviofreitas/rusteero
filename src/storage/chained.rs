//! Two-tier credential store: read/write through a `primary` backend, falling back to a
//! `fallback` backend.
//!
//! Ported from `ChainedStorage` (`eero-api`'s `src/eero/api/auth_storage.py:264-315`). See
//! `rusteero-context/claude/tasks/briefs/const.md` §7 for the full line-cited behaviour brief
//! this module implements, and the port plan §3.4 (decisions D-4, D-5) for the design
//! rationale. The shipped Python configuration wraps a `KeyringStorage` primary around a
//! `FileStorage` fallback (`auth_storage.py:318-344`'s `create_storage(use_keyring=True,
//! cookie_file=...)`), but this type is generic over any two [`CredentialStore`]
//! implementations, exactly like the Python constructor's own untyped `primary`/`fallback`
//! parameters (`auth_storage.py:270-278`).

use std::sync::Arc;

use crate::auth::Session;
use crate::error::StorageError;

use super::CredentialStore;

/// A [`CredentialStore`] that prefers a `primary` backend and falls back to a `fallback`
/// backend when the primary has, or reports, nothing usable.
///
/// `#[derive(Debug)]` is safe here without redaction: [`CredentialStore`] requires `Debug` of
/// every implementation (see the trait's own documentation), so both `primary` and `fallback`
/// are already responsible for redacting any credential they hold before this wrapper ever
/// touches them; this type has no session data of its own to leak.
#[derive(Debug)]
pub struct ChainedStore {
    primary: Arc<dyn CredentialStore>,
    fallback: Arc<dyn CredentialStore>,
}

impl ChainedStore {
    /// Builds a chained store over an explicit `primary` and `fallback` backend.
    ///
    /// Mirrors `ChainedStorage.__init__(primary, fallback)` (`auth_storage.py:270-278`): there
    /// are no default backends, both must be supplied explicitly.
    #[must_use]
    pub fn new(primary: Arc<dyn CredentialStore>, fallback: Arc<dyn CredentialStore>) -> Self {
        Self { primary, fallback }
    }

    /// Wraps a "both backends failed" pair of errors into a single [`StorageError`].
    ///
    /// Neither [`load`](CredentialStore::load) nor [`clear`](CredentialStore::clear) has a
    /// dedicated `StorageError` variant for "two backends failed at once" (the enum is
    /// `#[non_exhaustive]` and owned by `error.rs`), so this reuses
    /// [`StorageError::Backend`] with `backend: "chained"`, folding both underlying messages
    /// into one description. Neither message can contain a credential value, since
    /// [`StorageError::Backend::message`] itself is documented to never include one.
    fn both_failed(primary_err: &StorageError, fallback_err: &StorageError) -> StorageError {
        StorageError::Backend {
            backend: "chained".to_owned(),
            message: format!(
                "primary storage failed: {primary_err}; fallback storage also failed: \
                 {fallback_err}"
            ),
        }
    }
}

impl CredentialStore for ChainedStore {
    /// Loads from `primary`; if it has no usable session, loads from `fallback` and, on a hit,
    /// best-effort migrates the session back into `primary`.
    ///
    /// Mirrors `ChainedStorage.load()` (`auth_storage.py:280-293`):
    ///
    /// ```python
    /// credentials = await self._primary.load()
    /// if credentials.session_id:
    ///     return credentials
    /// credentials = await self._fallback.load()
    /// if credentials.session_id:
    ///     await self._primary.save(credentials)   # migrate up to primary
    /// return credentials
    /// ```
    ///
    /// "Usable" is decided with [`Session::is_valid`] (non-empty token *and* not expired)
    /// rather than Python's bare `if credentials.session_id:` truthiness check. Python can get
    /// away with a truthiness-only check because every Python backend's own `load()` already
    /// self-clears an expired session's `session_id` before returning it
    /// (`auth_storage.py:136-139,196-200`), so by the time `ChainedStorage.load()` inspects the
    /// result, an expired session is already indistinguishable from an absent one. Nothing in
    /// this trait's contract requires a Rust backend to do the same, so checking
    /// [`Session::is_valid`] here keeps `ChainedStore` correct regardless of whether a given
    /// backend performs that self-clear.
    ///
    /// Design decision beyond the literal Python source (there is no Python precedent for this
    /// branch, since every shipped Python backend's `load()` swallows all internal failures and
    /// never raises, per the behaviour brief §4/§5): if `primary.load()` itself returns `Err`,
    /// that is treated the same as "primary has no usable session" and this falls through to
    /// `fallback`, rather than propagating the error immediately. A primary storage backend
    /// that cannot even be read from (e.g. a keyring daemon that is not running) is exactly the
    /// situation a fallback exists to cover; only if `fallback.load()` *also* fails is the
    /// error propagated, since there is nowhere left to turn.
    ///
    /// Divergence from `eero-api`: Python's migration write at `auth_storage.py:291` is
    /// unguarded — a `primary.save()` failure there would propagate straight out of `load()` (a
    /// *read* operation failing because of a failed opportunistic write-back), see the
    /// behaviour brief's gotcha #8. This port treats the migration write as strictly
    /// best-effort: a failed migration is discarded (in place of Python's logging, which this
    /// crate does not perform in library code) and the session fetched from `fallback` is
    /// still returned. A read must never fail merely because its opportunistic write-back did.
    fn load(&self) -> Result<Session, StorageError> {
        let primary_usable = match self.primary.load() {
            Ok(session) if session.is_valid() => Some(session),
            Ok(_) | Err(_) => None,
        };
        if let Some(session) = primary_usable {
            return Ok(session);
        }

        let fallback_session = self.fallback.load()?;
        if fallback_session.is_valid() {
            // Best-effort migration: the outcome is intentionally discarded per this method's
            // own documentation above — a failed write-back must never fail this read.
            let _ = self.primary.save(&fallback_session);
        }

        Ok(fallback_session)
    }

    /// Saves to `primary`; only if `primary` reports an error does this also save to
    /// `fallback`.
    ///
    /// Mirrors `ChainedStorage.save()` (`auth_storage.py:295-310`):
    ///
    /// ```python
    /// try:
    ///     await self._primary.save(credentials)
    /// except Exception as e:
    ///     try:
    ///         await self._fallback.save(credentials)
    ///     except Exception as fallback_error:
    ///         _LOGGER.error("Both primary and fallback storage failed: %s", fallback_error)
    /// ```
    ///
    /// Divergence from `eero-api` (behaviour brief §7, gotcha #7): in the shipped Python
    /// configuration, `primary` is always a `KeyringStorage`, and `KeyringStorage.save()`
    /// (`auth_storage.py:147-155`) *never raises* — every internal failure is caught and only
    /// logged at DEBUG inside `KeyringStorage` itself. That makes the `except Exception` above
    /// unreachable in practice, so `fallback.save()` at `auth_storage.py:307` is dead code in
    /// the shipped configuration: a real keyring failure (e.g. no Secret Service/D-Bus running,
    /// the documented failure mode on headless Linux) silently drops the session with no
    /// fallback write at all, even though a working file fallback exists. This port's
    /// `KeyringStore::save` is designed to return `Err` on a genuine backend failure instead of
    /// swallowing it (see `CredentialStore::save`'s own documentation: unlike every Python
    /// backend, this port surfaces save failures as a typed error), so the fallback branch
    /// below actually activates. See this module's tests for coverage proving the fallback
    /// fires on a primary error.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] only if **both** `primary` and `fallback` fail to save;
    /// otherwise the error from a failed `primary` write is discarded once `fallback` succeeds,
    /// matching Python's swallow-on-recovery behaviour (there is no separate "primary failed
    /// but fallback succeeded" error to report — the session did end up durably stored).
    fn save(&self, session: &Session) -> Result<(), StorageError> {
        match self.primary.save(session) {
            Ok(()) => Ok(()),
            Err(primary_err) => match self.fallback.save(session) {
                Ok(()) => Ok(()),
                Err(fallback_err) => Err(Self::both_failed(&primary_err, &fallback_err)),
            },
        }
    }

    /// Clears both `primary` and `fallback` unconditionally.
    ///
    /// Mirrors `ChainedStorage.clear()` (`auth_storage.py:312-315`), which awaits both
    /// backends' `clear()` in sequence with no exception handling at all, relying on each
    /// backend's own `clear()` being swallow-all. This port cannot rely on that (per this
    /// trait's own `save`/`clear` documentation, a Rust backend may legitimately return `Err`),
    /// so the chosen semantics are: **both backends are always given a chance to clear** (a
    /// `primary` failure never skips the attempt on `fallback`, and vice versa — clearing
    /// credentials must be as thorough as possible), and an error is reported only if **both**
    /// fail. If only one backend fails, that is treated as an overall success, since the
    /// credential no longer exists in at least one place and — for the shipped
    /// keyring-primary/file-fallback configuration — the more sensitive of the two locations is
    /// no worse off than before.
    fn clear(&self) -> Result<(), StorageError> {
        let primary_result = self.primary.clear();
        let fallback_result = self.fallback.clear();
        match (primary_result, fallback_result) {
            (Ok(()), _) | (_, Ok(())) => Ok(()),
            (Err(primary_err), Err(fallback_err)) => {
                Err(Self::both_failed(&primary_err, &fallback_err))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{ChainedStore, CredentialStore};
    use crate::auth::Session;
    use crate::error::StorageError;
    use crate::storage::memory::MemoryStore;

    /// A [`CredentialStore`] test double that fails every operation. Not part of the public
    /// API — exists only to exercise [`ChainedStore`]'s fallback paths, which `MemoryStore`
    /// alone (never fails) cannot reach.
    #[derive(Debug, Default)]
    struct AlwaysFailsStore;

    impl AlwaysFailsStore {
        fn error() -> StorageError {
            StorageError::Backend {
                backend: "always-fails".to_owned(),
                message: "deliberate test failure".to_owned(),
            }
        }
    }

    impl CredentialStore for AlwaysFailsStore {
        fn load(&self) -> Result<Session, StorageError> {
            Err(Self::error())
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            Err(Self::error())
        }

        fn clear(&self) -> Result<(), StorageError> {
            Err(Self::error())
        }
    }

    /// A [`CredentialStore`] test double that loads as empty but fails to save. Isolates a
    /// migration *write* failure (`primary.save()` erroring during `load()`'s opportunistic
    /// migration) from a primary *read* failure, which [`AlwaysFailsStore`] cannot do since it
    /// fails both operations.
    #[derive(Debug, Default)]
    struct SaveFailsStore;

    impl CredentialStore for SaveFailsStore {
        fn load(&self) -> Result<Session, StorageError> {
            Ok(Session::empty())
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            Err(AlwaysFailsStore::error())
        }

        fn clear(&self) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn store(session: Option<&str>) -> Arc<MemoryStore> {
        let store = Arc::new(MemoryStore::new());
        if let Some(token) = session {
            store
                .save(&Session::from_token(token))
                .expect("save into a fresh MemoryStore never fails");
        }
        store
    }

    // ===================== load =====================

    #[test]
    fn load_returns_primary_session_without_needing_fallback() {
        let primary = store(Some("primary-token"));
        // If the implementation ever mistakenly consulted the fallback despite a usable
        // primary session, this failing double would surface it as a panic on `.expect(...)`.
        let fallback: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);

        let chained = ChainedStore::new(primary, fallback);
        let session = chained.load().expect("primary session is usable");
        assert_eq!(session.expose_token(), "primary-token");
    }

    #[test]
    fn load_falls_back_and_migrates_fallback_session_into_primary() {
        let primary = store(None);
        let fallback = store(Some("fallback-token"));

        let chained = ChainedStore::new(Arc::clone(&primary) as _, Arc::clone(&fallback) as _);
        let session = chained.load().expect("fallback session is usable");
        assert_eq!(session.expose_token(), "fallback-token");

        let migrated = primary.load().expect("load never fails");
        assert_eq!(
            migrated.expose_token(),
            "fallback-token",
            "a usable fallback session must be migrated into primary"
        );
    }

    #[test]
    fn load_returns_fallback_session_even_when_migration_fails() {
        let primary: Arc<dyn CredentialStore> = Arc::new(SaveFailsStore);
        let fallback = store(Some("fallback-token"));

        let chained = ChainedStore::new(primary, fallback);
        let session = chained
            .load()
            .expect("a failed migration must not fail the load");
        assert_eq!(session.expose_token(), "fallback-token");
    }

    #[test]
    fn load_falls_back_when_primary_load_itself_errors() {
        let primary: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let fallback = store(Some("fallback-token"));

        let chained = ChainedStore::new(primary, fallback);
        let session = chained
            .load()
            .expect("a primary read failure must fall through to fallback");
        assert_eq!(session.expose_token(), "fallback-token");
    }

    #[test]
    fn load_propagates_error_when_both_backends_fail() {
        let primary: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let fallback: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);

        let chained = ChainedStore::new(primary, fallback);
        let err = chained.load().expect_err("both backends failed");
        assert!(matches!(err, StorageError::Backend { .. }));
    }

    #[test]
    fn load_returns_empty_session_when_neither_backend_has_one() {
        let primary = store(None);
        let fallback = store(None);

        let chained = ChainedStore::new(primary, fallback);
        let session = chained
            .load()
            .expect("load never fails when both are empty");
        assert!(!session.is_valid());
        assert_eq!(session.expose_token(), "");
    }

    // ===================== save =====================

    #[test]
    fn save_writes_only_to_primary_when_it_succeeds() {
        let primary = store(None);
        let fallback = store(None);

        let chained = ChainedStore::new(Arc::clone(&primary) as _, Arc::clone(&fallback) as _);
        chained
            .save(&Session::from_token("tok"))
            .expect("primary save succeeds");

        assert_eq!(
            primary.load().expect("load never fails").expose_token(),
            "tok"
        );
        assert!(
            !fallback.load().expect("load never fails").is_valid(),
            "fallback must not be touched when primary succeeds"
        );
    }

    #[test]
    fn save_falls_back_when_primary_errors() {
        let primary: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let fallback = store(None);

        let chained = ChainedStore::new(primary, Arc::clone(&fallback) as _);
        chained
            .save(&Session::from_token("tok"))
            .expect("fallback save succeeds even though primary failed");

        assert_eq!(
            fallback.load().expect("load never fails").expose_token(),
            "tok"
        );
    }

    #[test]
    fn save_returns_error_when_both_backends_fail() {
        let primary: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let fallback: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);

        let chained = ChainedStore::new(primary, fallback);
        let err = chained
            .save(&Session::from_token("tok"))
            .expect_err("both backends failed");
        assert!(matches!(err, StorageError::Backend { .. }));
    }

    // ===================== clear =====================

    #[test]
    fn clear_clears_both_backends() {
        let primary = store(Some("primary-token"));
        let fallback = store(Some("fallback-token"));

        let chained = ChainedStore::new(Arc::clone(&primary) as _, Arc::clone(&fallback) as _);
        chained
            .clear()
            .expect("clear succeeds when both backends succeed");

        assert!(!primary.load().expect("load never fails").is_valid());
        assert!(!fallback.load().expect("load never fails").is_valid());
    }

    #[test]
    fn clear_succeeds_if_only_one_backend_fails() {
        let primary: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let fallback = store(Some("fallback-token"));

        let chained = ChainedStore::new(primary, Arc::clone(&fallback) as _);
        chained
            .clear()
            .expect("clearing is as thorough as possible: one success is enough");

        assert!(!fallback.load().expect("load never fails").is_valid());
    }

    #[test]
    fn clear_returns_error_only_when_both_backends_fail() {
        let primary: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let fallback: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);

        let chained = ChainedStore::new(primary, fallback);
        let err = chained.clear().expect_err("both backends failed");
        assert!(matches!(err, StorageError::Backend { .. }));
    }
}
