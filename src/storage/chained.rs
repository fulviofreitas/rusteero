//! Two-tier credential store: read/write through a `primary` backend, falling back to a
//! `fallback` backend.
//!
//! Ported from `ChainedStorage` (`eero-api`'s `src/eero/api/auth_storage.py:264-315`).
//! The shipped Python configuration wraps a `KeyringStorage` primary around a
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

    /// Wraps a "primary failed, fallback succeeded" [`clear`](CredentialStore::clear) outcome.
    ///
    /// `backend: "chained-primary"` names *which* backend still holds the credential a caller
    /// asked to destroy, so a caller (or its logs) can distinguish this from
    /// [`Self::fallback_only_failed`] and [`Self::both_failed`] without parsing the message.
    fn primary_only_failed(primary_err: &StorageError) -> StorageError {
        StorageError::Backend {
            backend: "chained-primary".to_owned(),
            message: format!(
                "primary storage failed to clear (fallback cleared successfully, but a copy \
                 of the credential may still be at rest in primary): {primary_err}"
            ),
        }
    }

    /// Wraps a "fallback failed, primary succeeded" [`clear`](CredentialStore::clear) outcome.
    ///
    /// See [`Self::primary_only_failed`] for why this is a distinct backend name rather than
    /// reusing [`Self::both_failed`].
    fn fallback_only_failed(fallback_err: &StorageError) -> StorageError {
        StorageError::Backend {
            backend: "chained-fallback".to_owned(),
            message: format!(
                "fallback storage failed to clear (primary cleared successfully, but a copy \
                 of the credential may still be at rest in fallback): {fallback_err}"
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
    /// never raises): if `primary.load()` itself returns `Err`,
    /// that is treated the same as "primary has no usable session" and this falls through to
    /// `fallback`, rather than propagating the error immediately. A primary storage backend
    /// that cannot even be read from (e.g. a keyring daemon that is not running) is exactly the
    /// situation a fallback exists to cover; only if `fallback.load()` *also* fails is the
    /// error propagated, since there is nowhere left to turn.
    ///
    /// Divergence from `eero-api`: Python's migration write at `auth_storage.py:291` is
    /// unguarded — a `primary.save()` failure there would propagate straight out of `load()` (a
    /// *read* operation failing because of a failed opportunistic write-back). This port treats the migration write as strictly
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
    /// Divergence from `eero-api`: in the shipped Python
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
    /// # Security fix
    ///
    /// Before writing to `fallback`, this also best-effort clears `primary` — discarding that
    /// `clear`'s own error — so a stale credential left behind by the failed `primary.save()`
    /// is invalidated rather than left readable. Without this, [`CredentialStore::load`]'s
    /// unconditional preference for `primary` (see this trait's own `load` docs above) would
    /// keep handing back the *old* token indefinitely once `primary` becomes reachable again —
    /// even though a newer credential (including the emptied "logged out" session written by a
    /// caller like `logout`/`refresh`) had already superseded it in `fallback`. The chain must
    /// never serve a credential a later write superseded.
    ///
    /// # Security fix
    ///
    /// After a *successful* `primary.save()`, this also best-effort clears `fallback` — the
    /// mirror image of the fix above. Without this, a fallback copy left behind by an earlier
    /// failed `primary.save()` (or simply primed directly, e.g. by a test, or by a prior process
    /// that only ever reached `fallback`) would keep sitting on disk/in the keyring indefinitely
    /// after a later, successful `primary` write superseded it — `ChainedStore` is the shipped
    /// configuration's only durable store, so a stray fallback copy
    /// is a stale, fully-valid plaintext (or keyring) credential with no way for a caller to know
    /// it is still there. This clear is best-effort and its own failure is discarded (logged at
    /// `DEBUG`) — it must never turn an otherwise-successful save into a reported failure.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] only if **both** `primary` and `fallback` fail to save;
    /// otherwise the error from a failed `primary` write is discarded once `fallback` succeeds,
    /// matching Python's swallow-on-recovery behaviour (there is no separate "primary failed
    /// but fallback succeeded" error to report — the session did end up durably stored).
    fn save(&self, session: &Session) -> Result<(), StorageError> {
        match self.primary.save(session) {
            Ok(()) => {
                if let Err(err) = self.fallback.clear() {
                    tracing::debug!(
                        error = %err,
                        "chained store: best-effort fallback clear after a successful primary \
                         save failed; a superseded credential may still be at rest in fallback"
                    );
                }
                Ok(())
            }
            Err(primary_err) => {
                // Best-effort: invalidate whatever `primary` was still holding so a later
                // `load()` (which unconditionally prefers `primary`) can never resurrect a
                // credential this write was meant to supersede. Its own failure is discarded —
                // if `primary` cannot be written to right now, it may well be unreachable for
                // clearing too, and either way the fallback write below must still happen.
                let _ = self.primary.clear();
                match self.fallback.save(session) {
                    Ok(()) => Ok(()),
                    Err(fallback_err) => Err(Self::both_failed(&primary_err, &fallback_err)),
                }
            }
        }
    }

    /// Clears both `primary` and `fallback` unconditionally, reporting an error if **either**
    /// fails.
    ///
    /// Mirrors `ChainedStorage.clear()` (`auth_storage.py:312-315`), which awaits both
    /// backends' `clear()` in sequence with no exception handling at all, relying on each
    /// backend's own `clear()` being swallow-all.
    ///
    /// # Security fix
    ///
    /// This method previously reported success as soon as *either* backend cleared
    /// successfully — the inverse of the guarantee a caller destroying a credential actually
    /// needs. `clear_auth_data()` makes no network call, so a live, unexpired token left behind
    /// in the backend that failed to clear would remain valid server-side for the rest of its
    /// natural (30-day) life while the caller believed it had been erased; if the surviving
    /// copy happened to be in `fallback`, the *next* [`CredentialStore::load`] would even
    /// migrate it back into `primary`, fully resurrecting the very session `clear()` was meant
    /// to destroy.
    ///
    /// Both backends are still always given the chance to clear — a `primary` failure never
    /// skips the attempt on `fallback`, and vice versa, so clearing remains as thorough as
    /// possible — but the overall result is now `Ok(())` **only when both succeed**. When
    /// exactly one backend fails, the returned [`StorageError::Backend`] names which one is
    /// still holding a copy of the credential via its `backend` field: `"chained-primary"`
    /// (primary failed, fallback cleared — see `primary_only_failed`) or `"chained-fallback"`
    /// (fallback failed, primary cleared — see `fallback_only_failed`); `"chained"` is reserved
    /// for the case where both backends failed (see `both_failed`). A caller can inspect this
    /// field to decide whether the surviving copy matters enough to retry or surface to the
    /// user.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Backend`] if either `primary` or `fallback` (or both) failed to
    /// clear; see the `backend` field discussion above for how to tell them apart.
    fn clear(&self) -> Result<(), StorageError> {
        // Both backends are always attempted, unconditionally and independently — never
        // short-circuited by the other's outcome — so clearing stays as thorough as possible
        // regardless of which one (if either) fails.
        let primary_result = self.primary.clear();
        let fallback_result = self.fallback.clear();
        match (primary_result, fallback_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(primary_err), Ok(())) => Err(Self::primary_only_failed(&primary_err)),
            (Ok(()), Err(fallback_err)) => Err(Self::fallback_only_failed(&fallback_err)),
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

    /// A [`CredentialStore`] test double that always fails to save but otherwise delegates to
    /// a real [`MemoryStore`] — isolates the guarantee that a failed `primary.save()` must clear
    /// whatever `primary` was still holding from [`SaveFailsStore`] above, which has no
    /// internal state of its own to observe being cleared.
    #[derive(Debug)]
    struct SaveFailsDelegatingStore {
        inner: Arc<MemoryStore>,
    }

    impl CredentialStore for SaveFailsDelegatingStore {
        fn load(&self) -> Result<Session, StorageError> {
            self.inner.load()
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            Err(AlwaysFailsStore::error())
        }

        fn clear(&self) -> Result<(), StorageError> {
            self.inner.clear()
        }
    }

    /// A [`CredentialStore`] test double that delegates `load`/`save` to a real [`MemoryStore`]
    /// but always fails to `clear` — isolates the best-effort discipline (a failed
    /// fallback `clear()` after a successful primary `save()` must never fail the save itself)
    /// from [`AlwaysFailsStore`], which has no internal state of its own to observe surviving.
    #[derive(Debug)]
    struct ClearFailsDelegatingStore {
        inner: Arc<MemoryStore>,
    }

    impl CredentialStore for ClearFailsDelegatingStore {
        fn load(&self) -> Result<Session, StorageError> {
            self.inner.load()
        }

        fn save(&self, session: &Session) -> Result<(), StorageError> {
            self.inner.save(session)
        }

        fn clear(&self) -> Result<(), StorageError> {
            Err(AlwaysFailsStore::error())
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
            "fallback stays empty when primary succeeds and fallback started empty"
        );
    }

    // ===================== save: stale fallback credential cleared on primary success =====================

    #[test]
    fn save_clears_a_previously_primed_fallback_once_primary_succeeds() {
        let primary = store(None);
        let fallback = store(Some("stale-fallback-token"));

        let chained = ChainedStore::new(Arc::clone(&primary) as _, Arc::clone(&fallback) as _);
        chained
            .save(&Session::from_token("new-token"))
            .expect("primary save succeeds");

        assert_eq!(
            primary.load().expect("load never fails").expose_token(),
            "new-token",
            "primary must hold the newly saved session"
        );
        assert!(
            !fallback.load().expect("load never fails").is_valid(),
            "a superseded fallback copy must be cleared once primary succeeds"
        );
    }

    #[test]
    fn save_still_succeeds_when_the_fallback_clear_itself_fails() {
        let inner_fallback = Arc::new(MemoryStore::new());
        inner_fallback
            .save(&Session::from_token("stale-fallback-token"))
            .expect("seed save never fails");

        let primary = store(None);
        let fallback: Arc<dyn CredentialStore> = Arc::new(ClearFailsDelegatingStore {
            inner: Arc::clone(&inner_fallback),
        });

        let chained = ChainedStore::new(Arc::clone(&primary) as _, fallback);
        chained
            .save(&Session::from_token("new-token"))
            .expect("a failed best-effort fallback clear must never fail the save itself");

        assert_eq!(
            primary.load().expect("load never fails").expose_token(),
            "new-token"
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

    // ===================== save: stale primary credential cleared on fallback =====================

    #[test]
    fn save_failure_on_primary_clears_the_stale_primary_credential_before_writing_fallback() {
        let inner_primary = Arc::new(MemoryStore::new());
        inner_primary
            .save(&Session::from_token("stale-token"))
            .expect("seed save never fails");
        let primary: Arc<dyn CredentialStore> = Arc::new(SaveFailsDelegatingStore {
            inner: Arc::clone(&inner_primary),
        });
        let fallback = store(None);

        let chained = ChainedStore::new(primary, Arc::clone(&fallback) as _);
        chained
            .save(&Session::from_token("new-token"))
            .expect("fallback save succeeds even though primary failed");

        // The stale primary entry must have been invalidated, not merely left in place —
        // otherwise a later reachable primary would keep serving it forever.
        assert!(
            !inner_primary.load().expect("load never fails").is_valid(),
            "a failed primary save must clear whatever primary was still holding"
        );

        // A subsequent chained load must not resurrect the stale primary credential: with
        // primary now empty, it must fall through to fallback and return the newer session.
        let loaded = chained.load().expect("load never fails");
        assert_eq!(
            loaded.expose_token(),
            "new-token",
            "load must never resurrect a credential a later write superseded"
        );
    }

    // ===================== clear =====================

    #[test]
    fn clear_succeeds_only_when_both_backends_succeed() {
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
    fn clear_attempts_both_backends_and_fails_when_only_primary_fails() {
        let primary: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let fallback = store(Some("fallback-token"));

        let chained = ChainedStore::new(primary, Arc::clone(&fallback) as _);
        let err = chained
            .clear()
            .expect_err("a caller destroying a credential must see that a copy survived (S1)");
        assert!(
            matches!(err, StorageError::Backend { ref backend, .. } if backend == "chained-primary")
        );

        // Fallback must still have been attempted (and succeeded) despite primary failing.
        assert!(!fallback.load().expect("load never fails").is_valid());
    }

    #[test]
    fn clear_attempts_both_backends_and_fails_when_only_fallback_fails() {
        let primary = store(Some("primary-token"));
        let fallback: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);

        let chained = ChainedStore::new(Arc::clone(&primary) as _, fallback);
        let err = chained
            .clear()
            .expect_err("a caller destroying a credential must see that a copy survived (S1)");
        assert!(
            matches!(err, StorageError::Backend { ref backend, .. } if backend == "chained-fallback")
        );

        // Primary must still have been attempted (and succeeded) despite fallback failing.
        assert!(!primary.load().expect("load never fails").is_valid());
    }

    #[test]
    fn clear_fails_when_both_backends_fail() {
        let primary: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
        let fallback: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);

        let chained = ChainedStore::new(primary, fallback);
        let err = chained.clear().expect_err("both backends failed");
        assert!(matches!(err, StorageError::Backend { ref backend, .. } if backend == "chained"));
    }
}
