//! Two-tier credential store: read/write through a `primary` backend, falling back to a
//! `fallback` backend.
//!
//! Ported from `ChainedStorage` (`eero-api`'s `src/eero/api/auth_storage.py:645-805` at
//! `v8.4.1`). The shipped Python configuration wraps a `KeyringStorage` primary around a
//! `FileStorage` fallback (`auth_storage.py:807-830`'s `create_storage(use_keyring=True,
//! cookie_file=...)`), but this type is generic over any two [`CredentialStore`]
//! implementations, exactly like the Python constructor's own untyped `primary`/`fallback`
//! parameters (`auth_storage.py:654-667`).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use secrecy::{ExposeSecret, SecretString};

use crate::auth::Session;
use crate::error::StorageError;

use super::CredentialStore;

/// A [`CredentialStore`] that prefers a `primary` backend and falls back to a `fallback`
/// backend when the primary has, or reports, nothing usable.
///
/// # Single writer, and whose record is whose
///
/// After a *verified* write to the primary, the chain retires the fallback record so the primary
/// is the only live copy, but only a record it knows is its own: this instance remembers the
/// session token it last loaded from, promoted out of, or wrote to the fallback, and clears the
/// fallback only if it still holds that token or the token just saved. Anything else was written
/// by someone else (another process sharing one cookie file, say, because its keyring was
/// unavailable) and is left in place (`auth_storage.py:757-778`). The remembered token is kept
/// in memory only, never logged, and is dropped once the fallback has been cleared.
///
/// The chain holds no lock of its own: each backend serialises itself, and a chained operation
/// is not atomic across the two (`auth_storage.py:645-652`).
///
/// `Debug` is written by hand: it prints the two backends, whose own `Debug` impls are
/// responsible for redacting anything they hold, and never the remembered token.
pub struct ChainedStore {
    primary: Arc<dyn CredentialStore>,
    fallback: Arc<dyn CredentialStore>,
    /// The session token this instance last loaded from, promoted out of, or wrote to the
    /// fallback (`ChainedStorage._fallback_token_seen`, `auth_storage.py:663-667`).
    fallback_token_seen: Mutex<Option<SecretString>>,
}

impl std::fmt::Debug for ChainedStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainedStore")
            .field("primary", &self.primary)
            .field("fallback", &self.fallback)
            .finish_non_exhaustive()
    }
}

impl ChainedStore {
    /// Builds a chained store over an explicit `primary` and `fallback` backend.
    ///
    /// Mirrors `ChainedStorage.__init__(primary, fallback)` (`auth_storage.py:654-667`): there
    /// are no default backends, both must be supplied explicitly.
    #[must_use]
    pub fn new(primary: Arc<dyn CredentialStore>, fallback: Arc<dyn CredentialStore>) -> Self {
        Self {
            primary,
            fallback,
            fallback_token_seen: Mutex::new(None),
        }
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

    /// The error for a save that neither backend retained.
    fn not_retained() -> StorageError {
        StorageError::NotRetained {
            backend: "chained".to_owned(),
        }
    }

    /// The remembered fallback token, poison-tolerant (it guards a plain value).
    fn seen(&self) -> MutexGuard<'_, Option<SecretString>> {
        self.fallback_token_seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Remembers `session`'s token as one this instance loaded from, promoted out of, or wrote
    /// to the fallback.
    fn remember_fallback_token(&self, session: &Session) {
        *self.seen() = Some(SecretString::from(session.expose_token().to_owned()));
    }

    /// Forgets the remembered fallback token, once the fallback has been cleared.
    fn forget_fallback_token(&self) {
        *self.seen() = None;
    }

    /// Reads `store` back and reports whether it holds `session`'s token.
    ///
    /// Ported from `_retains` (`auth_storage.py:169-187`): a read that fails counts as "not
    /// retained", because not every backend is guaranteed to swallow its own read errors. The
    /// error is logged at `DEBUG` (its text never carries a token).
    fn retains(store: &dyn CredentialStore, session: &Session) -> bool {
        match store.load() {
            Ok(loaded) => loaded.expose_token() == session.expose_token(),
            Err(err) => {
                tracing::debug!("Storage verification read failed: {err}");
                false
            }
        }
    }

    /// Promotes a session found only in the fallback into the primary and, once a read-back
    /// proves the primary holds it, removes the now-duplicate fallback copy.
    ///
    /// Best-effort: nothing here fails the load that triggered it. The fallback is cleared only
    /// after positive proof (the save succeeded *and* the primary reads back the same token),
    /// because deleting the only surviving copy on the strength of a write that was merely
    /// accepted would lose the credential. Ported from the promotion in
    /// `ChainedStorage.load()` (`auth_storage.py:686-699`).
    fn promote_into_primary(&self, session: &Session) {
        self.remember_fallback_token(session);
        match self.primary.save(session) {
            Ok(()) if Self::retains(self.primary.as_ref(), session) => {
                match self.fallback.clear() {
                    Ok(()) => self.forget_fallback_token(),
                    Err(err) => {
                        tracing::debug!("Could not clear the promoted fallback record: {err}");
                    }
                }
            }
            Ok(()) => {
                tracing::debug!(
                    "Primary storage did not retain the promoted record; fallback kept"
                );
            }
            Err(err) => tracing::debug!(
                "Primary storage did not retain the promoted record; fallback kept ({err})"
            ),
        }
    }

    /// Best-effort removal of a fallback record the primary has just superseded.
    ///
    /// Ported from `_clear_superseded_fallback` (`auth_storage.py:757-778`). The fallback is read
    /// first and cleared only if it holds a record this instance owns: the token it last
    /// remembered there, or the one just saved to the primary. A read that fails, an empty
    /// fallback, or a different token leaves it untouched, and nothing here fails the save.
    /// The remembered token is forgotten only after `clear` returns normally.
    fn clear_superseded_fallback(&self, saved: &Session) {
        let held = match self.fallback.load() {
            Ok(held) => held,
            Err(err) => {
                tracing::debug!("Could not clear superseded fallback record: {err}");
                return;
            }
        };
        if !held.is_valid() {
            return;
        }
        let ours = held.expose_token() == saved.expose_token()
            || self
                .seen()
                .as_ref()
                .is_some_and(|seen| seen.expose_secret() == held.expose_token());
        if !ours {
            tracing::debug!("Fallback holds a record this instance did not write; left in place");
            return;
        }
        match self.fallback.clear() {
            Ok(()) => self.forget_fallback_token(),
            Err(err) => tracing::debug!("Could not clear superseded fallback record: {err}"),
        }
    }
}

impl CredentialStore for ChainedStore {
    /// Loads from `primary`; if it has no usable session, loads from `fallback` and, on a hit,
    /// promotes the session into `primary` and retires the fallback copy.
    ///
    /// Mirrors `ChainedStorage.load()` (`auth_storage.py:669-701`):
    ///
    /// ```python
    /// credentials = await self._primary.load()
    /// if credentials.session_id:
    ///     return credentials
    /// credentials = await self._fallback.load()
    /// if credentials.session_id:
    ///     self._fallback_token_seen = credentials.session_id
    ///     await self._primary.save(credentials)
    ///     promoted = await self._primary.load()
    ///     if promoted.session_id == credentials.session_id:
    ///         await self._fallback.clear()
    ///         self._fallback_token_seen = None
    ///     else:
    ///         _LOGGER.debug("Primary storage did not retain the promoted record; fallback kept")
    /// return credentials
    /// ```
    ///
    /// The single-writer invariant: after a verified promotion at most one backend holds the live
    /// record, so a later credential-clearing write to the primary can never be "resurrected" by
    /// a stale copy still sitting in the fallback. The fallback is cleared only once a read-back
    /// proves the primary holds the record; if the promotion was not retained the fallback is
    /// kept (and the token stays remembered, so a later verified save can retire it).
    ///
    /// "Usable" is decided with [`Session::is_valid`] (non-empty token) like Python's bare
    /// `if credentials.session_id:` truthiness check.
    ///
    /// Design decision beyond the literal Python source (there is no Python precedent for this
    /// branch, since every shipped Python backend's `load()` swallows all internal failures and
    /// never raises): if `primary.load()` itself returns `Err`, that is treated the same as
    /// "primary has no usable session" and this falls through to `fallback`, rather than
    /// propagating the error immediately. A primary storage backend that cannot even be read
    /// from (e.g. a keyring daemon that is not running) is exactly the situation a fallback
    /// exists to cover; only if `fallback.load()` *also* fails is the error propagated, since
    /// there is nowhere left to turn.
    ///
    /// Divergence from `eero-api`: Python's promotion write is unguarded against a raising
    /// backend. This port treats it as strictly best-effort: a failed promotion is logged at
    /// `DEBUG` and the session fetched from `fallback` is still returned. A read must never fail
    /// merely because its opportunistic write-back did. The fallback is also cleared only when
    /// the promotion save itself succeeded, not merely when a read-back matches.
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
            self.promote_into_primary(&fallback_session);
        }

        Ok(fallback_session)
    }

    /// Saves to `primary`; only if `primary` fails, or does not verifiably retain the record,
    /// does this also save to `fallback`.
    ///
    /// Mirrors `ChainedStorage.save()` (`auth_storage.py:703-755`):
    ///
    /// ```python
    /// try:
    ///     await self._primary.save(credentials)
    /// except Exception as e:
    ///     _LOGGER.debug("Primary storage save failed, trying fallback: %s", e)
    /// else:
    ///     if await _retains(self._primary, credentials):
    ///         await self._clear_superseded_fallback(credentials)
    ///         return True
    ///     _LOGGER.debug("Primary storage did not retain the saved record; trying fallback")
    /// try:
    ///     await self._fallback.save(credentials)
    /// except Exception as fallback_error:
    ///     _LOGGER.error("Both primary and fallback storage failed: %s", fallback_error)
    ///     return False
    /// retained = await _retains(self._fallback, credentials)
    /// if retained:
    ///     self._fallback_token_seen = credentials.session_id
    /// return retained
    /// ```
    ///
    /// A primary that accepts a write and retains nothing (a no-op keyring, or a third-party
    /// store that reports `Ok` without persisting) is indistinguishable from a working one by its
    /// return value alone, so a successful primary save is verified with a read-back (`load`),
    /// and only a verified write skips the fallback. A read-back that fails counts as not
    /// retained. After a verified primary write, the fallback record is retired if it is one this
    /// instance owns (see the type docs): a record someone else left there is left alone, and a
    /// failing read or clear of the fallback never fails the save.
    ///
    /// # Security: a stale primary credential is invalidated first
    ///
    /// Before writing to `fallback`, this also best-effort clears `primary` — discarding that
    /// `clear`'s own error — so a stale credential left behind by the failed `primary.save()` is
    /// invalidated rather than left readable. Without this, [`CredentialStore::load`]'s
    /// unconditional preference for `primary` would keep handing back the *old* token
    /// indefinitely once `primary` becomes reachable again — even though a newer credential
    /// (including the emptied "logged out" session written by a caller like `logout`/`refresh`)
    /// had already superseded it in `fallback`. The chain must never serve a credential a later
    /// write superseded.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Backend`] (`backend: "chained"`) if **both** `primary` and
    /// `fallback` fail to save, and [`StorageError::NotRetained`] (`backend: "chained"`) if the
    /// fallback accepted the save but a read-back did not find the record, i.e. no backend was
    /// read back holding the token. Otherwise the error from a failed `primary` write is
    /// discarded once `fallback` succeeds, matching Python's swallow-on-recovery behaviour
    /// (there is no separate "primary failed but fallback succeeded" error to report — the
    /// session did end up durably stored).
    fn save(&self, session: &Session) -> Result<(), StorageError> {
        let primary_err = match self.primary.save(session) {
            Ok(()) => {
                if Self::retains(self.primary.as_ref(), session) {
                    tracing::debug!("Saved to primary storage");
                    self.clear_superseded_fallback(session);
                    return Ok(());
                }
                tracing::debug!("Primary storage did not retain the saved record; trying fallback");
                StorageError::NotRetained {
                    backend: "primary".to_owned(),
                }
            }
            Err(err) => {
                tracing::debug!("Primary storage save failed, trying fallback: {err}");
                err
            }
        };

        // Best-effort: invalidate whatever `primary` was still holding so a later `load()`
        // (which unconditionally prefers `primary`) can never resurrect a credential this write
        // was meant to supersede. Its own failure is discarded — if `primary` cannot be written
        // to right now, it may well be unreachable for clearing too, and either way the fallback
        // write below must still happen.
        let _ = self.primary.clear();
        if let Err(fallback_err) = self.fallback.save(session) {
            return Err(Self::both_failed(&primary_err, &fallback_err));
        }
        tracing::debug!("Saved to fallback storage");
        if Self::retains(self.fallback.as_ref(), session) {
            self.remember_fallback_token(session);
            Ok(())
        } else {
            Err(Self::not_retained())
        }
    }

    /// Clears both `primary` and `fallback` unconditionally, reporting an error if **either**
    /// fails.
    ///
    /// Mirrors `ChainedStorage.clear()` (`auth_storage.py:780-805`), which always attempts both
    /// backends, logs each failure at `DEBUG` ("Error clearing a storage backend: ..."), logs
    /// one fixed `WARN` if any backend failed, and re-raises only when *both* did.
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
    /// Both backends are always given the chance to clear — a `primary` failure never skips the
    /// attempt on `fallback`, and vice versa, so clearing remains as thorough as possible — but
    /// the overall result is `Ok(())` **only when both succeed**. When exactly one backend fails,
    /// the returned [`StorageError::Backend`] names which one is still holding a copy of the
    /// credential via its `backend` field: `"chained-primary"` (primary failed, fallback
    /// cleared — see `primary_only_failed`) or `"chained-fallback"` (fallback failed, primary
    /// cleared — see `fallback_only_failed`); `"chained"` is reserved for the case where both
    /// backends failed (see `both_failed`). A caller can inspect this field to decide whether
    /// the surviving copy matters enough to retry or surface to the user.
    ///
    /// # Divergence from eero-api
    ///
    /// Python swallows a single failing backend (the `WARN` above is its only signal) and
    /// raises only when both fail, so `logout()` never fails because one backend did
    /// (`auth_storage.py:780-805`). This method keeps its `Err` for a single failure, so a
    /// caller destroying a credential can see that a copy survived, and adds Python's fixed
    /// `WARN` on top. The transport's default policy still swallows the error when it clears the
    /// session on logout, so the observable outcome there is Python's.
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
        for err in [&primary_result, &fallback_result]
            .into_iter()
            .filter_map(|result| result.as_ref().err())
        {
            tracing::debug!("Error clearing a storage backend: {err}");
        }
        if primary_result.is_err() || fallback_result.is_err() {
            tracing::warn!(
                "Could not clear every credential storage backend; a stored session may \
                 remain in the one that failed"
            );
        }
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use tracing::Level;

    use super::{ChainedStore, CredentialStore};
    use crate::auth::Session;
    use crate::error::StorageError;
    use crate::storage::memory::MemoryStore;
    use crate::storage::test_support::capture_logs;

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
    /// a real [`MemoryStore`] — isolates a failed `primary.save()` clearing whatever `primary`
    /// was still holding from [`SaveFailsStore`] above, which has no internal state of its own
    /// to observe being cleared.
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

    /// A [`CredentialStore`] over a [`MemoryStore`] whose behaviour can be switched at runtime
    /// and which counts the calls it receives. `drop_writes` makes it accept every save and
    /// retain nothing, like a keyring backend that silently loses writes.
    #[derive(Debug, Default)]
    struct FlakyStore {
        inner: MemoryStore,
        fail_load: AtomicBool,
        fail_save: AtomicBool,
        fail_clear: AtomicBool,
        drop_writes: AtomicBool,
        saves: AtomicUsize,
        clears: AtomicUsize,
    }

    impl FlakyStore {
        fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        fn seeded(token: &str) -> Arc<Self> {
            let store = Self::new();
            store
                .inner
                .save(&Session::from_token(token))
                .expect("save into a fresh MemoryStore never fails");
            store
        }

        /// Accepts every save and retains nothing.
        fn null() -> Arc<Self> {
            let store = Self::new();
            store.drop_writes.store(true, Ordering::SeqCst);
            store
        }

        fn set(flag: &AtomicBool, on: bool) {
            flag.store(on, Ordering::SeqCst);
        }

        /// The token currently held, bypassing every failure switch.
        fn held(&self) -> String {
            self.inner
                .load()
                .expect("in-memory load cannot fail")
                .expose_token()
                .to_owned()
        }

        fn saves(&self) -> usize {
            self.saves.load(Ordering::SeqCst)
        }

        fn clears(&self) -> usize {
            self.clears.load(Ordering::SeqCst)
        }
    }

    impl CredentialStore for FlakyStore {
        fn load(&self) -> Result<Session, StorageError> {
            if self.fail_load.load(Ordering::SeqCst) {
                return Err(AlwaysFailsStore::error());
            }
            self.inner.load()
        }

        fn save(&self, session: &Session) -> Result<(), StorageError> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            if self.fail_save.load(Ordering::SeqCst) {
                return Err(AlwaysFailsStore::error());
            }
            if self.drop_writes.load(Ordering::SeqCst) {
                return Ok(());
            }
            self.inner.save(session)
        }

        fn clear(&self) -> Result<(), StorageError> {
            self.clears.fetch_add(1, Ordering::SeqCst);
            if self.fail_clear.load(Ordering::SeqCst) {
                return Err(AlwaysFailsStore::error());
            }
            self.inner.clear()
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

    fn chain(primary: &Arc<FlakyStore>, fallback: &Arc<FlakyStore>) -> ChainedStore {
        ChainedStore::new(Arc::clone(primary) as _, Arc::clone(fallback) as _)
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
    fn load_clears_the_fallback_after_a_verified_promotion() {
        let primary = FlakyStore::new();
        let fallback = FlakyStore::seeded("fallback-token");

        let session = chain(&primary, &fallback).load().expect("load succeeds");

        assert_eq!(session.expose_token(), "fallback-token");
        assert_eq!(primary.held(), "fallback-token");
        assert_eq!(
            fallback.held(),
            "",
            "after a verified promotion the primary is the sole owner"
        );
    }

    #[test]
    fn load_returns_fallback_session_even_when_migration_fails() {
        let primary: Arc<dyn CredentialStore> = Arc::new(SaveFailsStore);
        let fallback = store(Some("fallback-token"));

        let chained = ChainedStore::new(primary, Arc::clone(&fallback) as _);
        let session = chained
            .load()
            .expect("a failed migration must not fail the load");
        assert_eq!(session.expose_token(), "fallback-token");
        assert_eq!(
            fallback.load().expect("load never fails").expose_token(),
            "fallback-token",
            "the only surviving copy is kept when the promotion failed"
        );
    }

    #[test]
    fn load_keeps_the_fallback_when_promotion_is_not_retained() {
        let (_guard, logs) = capture_logs();
        let primary = FlakyStore::null();
        let fallback = FlakyStore::seeded("fallback-token");

        let session = chain(&primary, &fallback)
            .load()
            .expect("an unretained promotion must not fail the load");

        assert_eq!(session.expose_token(), "fallback-token");
        assert_eq!(fallback.held(), "fallback-token");
        assert_eq!(fallback.clears(), 0, "the fallback is never cleared");
        assert_eq!(
            logs.count(
                Level::DEBUG,
                "Primary storage did not retain the promoted record; fallback kept"
            ),
            1
        );
        assert!(!logs.mentions("fallback-token"));
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

    #[test]
    fn fallback_loaded_by_this_instance_is_retired_by_a_later_verified_save() {
        let primary = FlakyStore::new();
        let fallback = FlakyStore::seeded("first_token");
        let chained = chain(&primary, &fallback);

        // The primary cannot take the promotion, so the fallback copy is kept...
        FlakyStore::set(&primary.fail_save, true);
        let loaded = chained.load().expect("load succeeds");
        assert_eq!(loaded.expose_token(), "first_token");
        assert_eq!(fallback.held(), "first_token");

        // ...but this instance now owns it, so a later verified save retires it even though the
        // token saved is a different one.
        FlakyStore::set(&primary.fail_save, false);
        chained
            .save(&Session::from_token("second_token"))
            .expect("save succeeds");
        assert_eq!(primary.held(), "second_token");
        assert_eq!(
            fallback.held(),
            "",
            "the fallback this instance loaded is retired"
        );
    }

    #[test]
    fn a_failed_fallback_clear_after_promotion_keeps_the_token_for_a_later_save() {
        let primary = FlakyStore::new();
        let fallback = FlakyStore::seeded("first_token");
        let chained = chain(&primary, &fallback);

        FlakyStore::set(&fallback.fail_clear, true);
        chained.load().expect("load succeeds");
        assert_eq!(primary.held(), "first_token");
        assert_eq!(fallback.held(), "first_token", "the clear failed");

        FlakyStore::set(&fallback.fail_clear, false);
        chained
            .save(&Session::from_token("second_token"))
            .expect("save succeeds");
        assert_eq!(
            fallback.held(),
            "",
            "the remembered token survived the failed clear and retires the fallback now"
        );
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

    #[test]
    fn save_falls_back_when_primary_accepts_but_does_not_retain() {
        let primary = FlakyStore::null();
        let fallback = FlakyStore::new();

        chain(&primary, &fallback)
            .save(&Session::from_token("fresh_token"))
            .expect("the fallback retains it");

        assert_eq!(fallback.held(), "fresh_token");
    }

    #[test]
    fn unverified_primary_never_clears_the_fallback() {
        let primary = FlakyStore::null();
        let fallback = FlakyStore::seeded("fresh_token");

        chain(&primary, &fallback)
            .save(&Session::from_token("fresh_token"))
            .expect("the fallback retains it");

        assert_eq!(fallback.held(), "fresh_token");
        assert_eq!(
            fallback.clears(),
            0,
            "a primary write that was not read back must not retire the fallback"
        );
    }

    #[test]
    fn a_primary_whose_read_back_errors_after_an_ok_save_falls_back() {
        let (_guard, logs) = capture_logs();
        let primary = FlakyStore::new();
        FlakyStore::set(&primary.fail_load, true);
        let fallback = FlakyStore::new();

        chain(&primary, &fallback)
            .save(&Session::from_token("fresh_token"))
            .expect("a failed verification read counts as not retained, so the fallback is used");

        assert_eq!(fallback.held(), "fresh_token");
        assert_eq!(
            logs.count(Level::DEBUG, "Storage verification read failed"),
            1
        );
        assert!(!logs.mentions("fresh_token"));
    }

    #[test]
    fn save_reports_not_retained_when_no_backend_retains() {
        let primary = FlakyStore::null();
        let fallback = FlakyStore::null();

        let err = chain(&primary, &fallback)
            .save(&Session::from_token("live_token"))
            .expect_err("nothing retained the record");

        assert!(
            matches!(&err, StorageError::NotRetained { backend } if backend == "chained"),
            "got {err:?}"
        );
        assert!(!err.to_string().contains("live_token"));
    }

    #[test]
    fn save_reports_not_retained_when_the_fallback_cannot_be_read_back() {
        let primary = FlakyStore::null();
        let fallback = FlakyStore::new();
        FlakyStore::set(&fallback.fail_load, true);

        let err = chain(&primary, &fallback)
            .save(&Session::from_token("live_token"))
            .expect_err("the fallback write could not be verified");

        assert!(matches!(err, StorageError::NotRetained { ref backend } if backend == "chained"));
    }

    #[test]
    fn save_reports_both_failed_when_an_unverified_primary_meets_a_failing_fallback() {
        let primary = FlakyStore::null();
        let fallback = FlakyStore::new();
        FlakyStore::set(&fallback.fail_save, true);

        let err = chain(&primary, &fallback)
            .save(&Session::from_token("live_token"))
            .expect_err("neither backend took the record");

        match err {
            StorageError::Backend { backend, message } => {
                assert_eq!(backend, "chained");
                assert!(message.contains("did not retain"), "got {message}");
                assert!(message.contains("deliberate test failure"), "got {message}");
                assert!(!message.contains("live_token"));
            }
            other => panic!("expected the chained both-failed error, got {other:?}"),
        }
    }

    #[test]
    fn fallback_this_instance_wrote_is_retired_after_the_primary_recovers() {
        let primary = FlakyStore::new();
        FlakyStore::set(&primary.fail_save, true);
        let fallback = FlakyStore::new();
        let chained = chain(&primary, &fallback);

        chained
            .save(&Session::from_token("first_token"))
            .expect("the fallback takes it");
        assert_eq!(fallback.held(), "first_token");

        FlakyStore::set(&primary.fail_save, false);
        chained
            .save(&Session::from_token("second_token"))
            .expect("the recovered primary takes it");
        assert_eq!(primary.held(), "second_token");
        assert_eq!(
            fallback.held(),
            "",
            "the fallback record this instance wrote is retired"
        );

        // The retired fallback cannot be resurrected once the primary is wiped.
        primary.clear().expect("clear succeeds");
        assert!(!chained.load().expect("load succeeds").is_valid());
    }

    // ===================== save: ownership of the fallback record =====================

    #[test]
    fn other_process_session_in_the_fallback_is_left_alone() {
        let (_guard, logs) = capture_logs();
        let primary = FlakyStore::new();
        let fallback = FlakyStore::seeded("other_process_token");

        chain(&primary, &fallback)
            .save(&Session::from_token("my_token"))
            .expect("primary save succeeds");

        assert_eq!(primary.held(), "my_token");
        assert_eq!(
            fallback.held(),
            "other_process_token",
            "a record this instance did not write is not ours to delete"
        );
        assert_eq!(fallback.clears(), 0);
        assert_eq!(
            logs.count(
                Level::DEBUG,
                "Fallback holds a record this instance did not write; left in place"
            ),
            1
        );
        assert!(!logs.mentions("other_process_token") && !logs.mentions("my_token"));
    }

    #[test]
    fn duplicate_of_the_saved_session_in_the_fallback_is_cleared() {
        let primary = FlakyStore::new();
        let fallback = FlakyStore::seeded("same_token");

        chain(&primary, &fallback)
            .save(&Session::from_token("same_token"))
            .expect("primary save succeeds");

        assert_eq!(primary.held(), "same_token");
        assert_eq!(
            fallback.held(),
            "",
            "a duplicate of the saved session is retired"
        );
    }

    #[test]
    fn unreadable_fallback_is_left_alone() {
        let (_guard, logs) = capture_logs();
        let primary = FlakyStore::new();
        let fallback = FlakyStore::seeded("same_token");
        FlakyStore::set(&fallback.fail_load, true);

        chain(&primary, &fallback)
            .save(&Session::from_token("same_token"))
            .expect("an unreadable fallback never fails the save");

        assert_eq!(
            fallback.clears(),
            0,
            "nothing is cleared that could not be read"
        );
        assert_eq!(
            logs.count(Level::DEBUG, "Could not clear superseded fallback record"),
            1
        );
        assert!(logs.messages(Level::WARN).is_empty());
    }

    #[test]
    fn failing_fallback_clear_never_escapes_save() {
        let (_guard, logs) = capture_logs();
        let primary = FlakyStore::new();
        let fallback = FlakyStore::seeded("same_token");
        FlakyStore::set(&fallback.fail_clear, true);

        chain(&primary, &fallback)
            .save(&Session::from_token("same_token"))
            .expect("a failed best-effort fallback clear must never fail the save itself");

        assert_eq!(primary.held(), "same_token");
        assert_eq!(fallback.clears(), 1, "the clear was attempted");
        assert_eq!(
            logs.count(Level::DEBUG, "Could not clear superseded fallback record"),
            1
        );
        assert!(logs.messages(Level::WARN).is_empty());
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
        assert!(matches!(err, StorageError::Backend { ref backend, .. } if backend == "chained"));
    }

    // ===================== save: a failed primary save invalidates the stale primary =====================

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
            .expect_err("a caller destroying a credential must see that a copy survived");
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
            .expect_err("a caller destroying a credential must see that a copy survived");
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

    // ===================== clear: logging =====================

    const CLEAR_WARNING: &str = "Could not clear every credential storage backend; a stored \
         session may remain in the one that failed";

    #[test]
    fn clear_logs_one_fixed_warning_when_only_the_primary_fails() {
        let (_guard, logs) = capture_logs();
        let primary = FlakyStore::seeded("primary_token");
        FlakyStore::set(&primary.fail_clear, true);
        let fallback = FlakyStore::seeded("fallback_token");

        chain(&primary, &fallback)
            .clear()
            .expect_err("the primary failed to clear");

        assert_eq!(logs.messages(Level::WARN), vec![CLEAR_WARNING.to_owned()]);
        assert_eq!(
            logs.count(Level::DEBUG, "Error clearing a storage backend"),
            1,
            "each failing backend is logged at DEBUG"
        );
        assert_eq!(fallback.held(), "", "the fallback was still cleared");
        assert!(!logs.mentions("primary_token") && !logs.mentions("fallback_token"));
    }

    #[test]
    fn clear_logs_one_fixed_warning_when_both_backends_fail() {
        let (_guard, logs) = capture_logs();
        let primary = FlakyStore::new();
        FlakyStore::set(&primary.fail_clear, true);
        let fallback = FlakyStore::new();
        FlakyStore::set(&fallback.fail_clear, true);

        chain(&primary, &fallback)
            .clear()
            .expect_err("both failed to clear");

        assert_eq!(logs.messages(Level::WARN), vec![CLEAR_WARNING.to_owned()]);
        assert_eq!(
            logs.count(Level::DEBUG, "Error clearing a storage backend"),
            2
        );
    }

    #[test]
    fn clear_logs_no_warning_when_both_backends_succeed() {
        let (_guard, logs) = capture_logs();
        let primary = FlakyStore::seeded("primary_token");
        let fallback = FlakyStore::seeded("fallback_token");

        chain(&primary, &fallback).clear().expect("clear succeeds");

        assert!(logs.messages(Level::WARN).is_empty());
        assert_eq!(
            logs.count(Level::DEBUG, "Error clearing a storage backend"),
            0
        );
    }

    // ===================== Debug =====================

    #[test]
    fn debug_shows_both_backends_and_never_the_remembered_token() {
        let primary = FlakyStore::new();
        FlakyStore::set(&primary.fail_save, true);
        let fallback = FlakyStore::seeded("remembered_secret");
        let chained = chain(&primary, &fallback);
        chained.load().expect("load succeeds");

        let debug = format!("{chained:?}");

        assert!(debug.contains("ChainedStore"));
        assert!(debug.contains("FlakyStore"));
        assert!(
            !debug.contains("remembered_secret"),
            "the token this instance remembers must never be printed: {debug}"
        );
        assert_eq!(primary.saves(), 1, "the promotion was attempted");
    }
}
