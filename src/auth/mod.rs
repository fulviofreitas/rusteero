//! Authentication: the session credential value ([`Session`]), its on-disk storage
//! representation, the interactive login handshake ([`flow`]), and the network-facing
//! authentication API ([`AuthApi`]).
//!
//! Ported from `eero-api`'s `src/eero/api/auth.py`. See
//! `.claude/tasks/briefs/auth.md` for the full behaviour brief this module implements.
pub mod flow;
pub mod session;

pub use session::Session;

use std::sync::Arc;

use serde_json::json;

use crate::envelope::Envelope;
use crate::error::{Error, StorageError};
use crate::routes;
use crate::transport::{StorageFailures, Transport};

/// The exact validation message Python raises for an empty token
/// (`exceptions.py:85`, called with `("token", "must be a non-empty string")` at
/// `api/auth.py:407-408`).
const EMPTY_TOKEN_MESSAGE: &str = "must be a non-empty string";

/// The network-facing half of `eero-api`'s `AuthAPI` (`src/eero/api/auth.py`): everything except
/// the interactive login handshake itself, which lives in [`flow`] as a separable type-state pair
/// ([`flow::LoginFlow`] / [`flow::PendingLogin`]).
///
/// Build one with [`AuthApi::new`], wrapping any already-configured [`Transport`] — the same
/// `Transport` a [`flow::LoginFlow`] can be pointed at, so a `Session` obtained from
/// [`flow::PendingLogin::verify`] and handed to `Transport::set_session` is immediately usable
/// here too. `AuthApi` holds the `Transport` behind an `Arc` internally so that
/// [`AuthApi::logout`] can move a handle onto a `tokio::task::spawn_blocking` task for its
/// credential-store write without requiring `Transport` itself to be `Clone`.
#[derive(Debug)]
pub struct AuthApi {
    transport: Arc<Transport>,
}

impl AuthApi {
    /// Wraps `transport` as an `AuthApi`.
    #[must_use]
    pub fn new(transport: Transport) -> Self {
        Self {
            transport: Arc::new(transport),
        }
    }

    /// Borrows the underlying [`Transport`], for callers (e.g. the not-yet-built `EeroApi`
    /// aggregator, phase 3) that need to issue their own requests through the same transport this
    /// `AuthApi` uses.
    #[must_use]
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Whether a session is configured, has a non-empty token, and has not passed its
    /// client-fabricated expiry.
    ///
    /// Ported from the `is_authenticated` property (`api/auth.py:51-62`; brief lines 286-296):
    /// this is a purely local check, delegating entirely to [`Transport::is_authenticated`] (in
    /// turn `Session::is_valid`) — it never makes a network call and never attempts a refresh,
    /// matching Python's own property exactly (the only side effect Python's version has beyond
    /// this crate's is a `DEBUG` log line on the expired-but-present case, which carries no
    /// observable behaviour worth reproducing).
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.transport.is_authenticated()
    }

    /// Returns a snapshot of the currently configured session, if any.
    ///
    /// A thin wrapper over [`Transport::session`]; see that method's docs.
    #[must_use]
    pub fn session(&self) -> Option<Session> {
        self.transport.session()
    }

    /// Logs the current session out.
    ///
    /// Ported from `logout()` (`api/auth.py:239-277`; see `.claude/tasks/briefs/auth.md` lines
    /// 171-202 for the full behaviour brief). Sends `POST` [`crate::routes::LOGOUT`] with body
    /// `{}` (`auth.py:256`, "Empty payload for logout"), through [`Transport::send`] — which
    /// already implements the "not authenticated" precondition (`auth.py:248-250`) and every
    /// status-to-error mapping a real logout response can produce, so no separate guard is
    /// needed here.
    ///
    /// # Divergence from eero-api (`auth.py:239-277`)
    ///
    /// Python's cleanup (`clear_all()` plus persisting the cleared credentials) only runs when
    /// the network call raised nothing, or raised the equivalent of [`Error::Authentication`] (a
    /// 401, treated as "already logged out server-side") or a generic [`Error::Api`]
    /// (`auth.py:259-266`): a 429 ([`Error::RateLimit`]) or a network/timeout failure
    /// ([`Error::Network`] / [`Error::Timeout`]) is **not** caught by any of `logout()`'s three
    /// `except` clauses and propagates straight out, **skipping** the cleanup block entirely
    /// (`.claude/tasks/briefs/auth.md` lines 189-193) — contradicting both Python's own "Always
    /// clear local credentials regardless of API response" comment (`auth.py:269`) and this
    /// port's plan. `rusteero` always clears the in-memory session and persists that clear to the
    /// credential store, regardless of the outcome of the network call (including the local
    /// "not authenticated" precondition failure, treated here as just another outcome), and only
    /// then returns that outcome unmodified — leaving a live token behind after a failed logout
    /// is the strictly less safe choice, and matches Python's own stated intent even though its
    /// implementation does not honour it.
    ///
    /// # Errors
    ///
    /// Propagates whatever [`Transport::send`] produces, including
    /// `Error::Authentication("Not authenticated")` if no valid session is configured, before any
    /// network call is made. The in-memory session is always cleared regardless of the network
    /// outcome or of whether persisting that clear succeeds (decision D-13) — but what this
    /// method *returns* when persistence fails now depends on the configured
    /// [`crate::transport::StorageFailures`] policy (security finding T2, fixed after a review
    /// found the previous version silently discarded a `Fatal`-policy storage failure here,
    /// leaving an operator who explicitly opted into "storage failures are fatal" with no
    /// programmatic way to learn the at-rest copy still held the old session):
    ///
    /// - If the network call succeeded but the configured store failed to persist the clear
    ///   under [`crate::transport::StorageFailures::Fatal`], this returns that `Error::Storage`
    ///   instead of the network `Ok`.
    /// - If the network call itself failed, that error is always what is returned — even if the
    ///   storage cleanup also failed under `Fatal` — since it is the more actionable failure and
    ///   the storage failure is already logged by this method's internal cleanup helper.
    /// - Under the default [`crate::transport::StorageFailures::Warn`], a storage failure never
    ///   changes what this method returns, matching the previous behaviour exactly.
    pub async fn logout(&self) -> Result<Envelope, Error> {
        let outcome = self
            .transport
            .send(&routes::LOGOUT, &[], Some(json!({})))
            .await;

        // Divergence from eero-api (auth.py:239-277): see this method's doc comment for the full
        // citation. Clear the in-memory session and persist that clear unconditionally, on every
        // outcome, rather than reproducing Python's exception-hierarchy gap that skips cleanup on
        // a 429 or a network failure.
        let cleanup = self.clear_local_and_store().await;

        match (outcome, cleanup) {
            (Ok(envelope), Ok(())) => Ok(envelope),
            // Security finding T2: a successful logout call must not hide a `Fatal`-policy
            // storage failure — see this method's doc comment.
            (Ok(_), Err(storage_err)) => Err(storage_err),
            // The network/auth outcome is the more actionable failure; return it even if the
            // storage cleanup also failed (already logged by `clear_local_and_store`) rather than
            // masking the reason the request itself failed with a storage error.
            (Err(network_err), Ok(()) | Err(_)) => Err(network_err),
        }
    }

    /// Attempts to refresh the current session.
    ///
    /// A thin wrapper over [`Transport::refresh_session`]; see that method's docs for the full
    /// behaviour, ported from `refresh_session()` (`api/auth.py:279-340`; brief lines 204-260).
    ///
    /// # Errors
    ///
    /// See [`Transport::refresh_session`].
    pub async fn refresh_session(&self) -> Result<bool, Error> {
        self.transport.refresh_session().await
    }

    /// Checks whether the current session is usable, without attempting a network refresh.
    ///
    /// Ported from `ensure_authenticated()` (`api/auth.py:342-360`; brief lines 262-284):
    /// delegates entirely to [`AuthApi::is_authenticated`], returning
    /// `Error::Authentication("Not authenticated")` when it is `false`. Python's version also has
    /// a branch that attempts `refresh_session()` when a session is present but has just passed
    /// its expiry at the exact instant of the check (`auth.py:352-358`) — the behaviour brief
    /// establishes that branch is logically unreachable in practice, since a normal login/verify
    /// never yields a refresh token (`auth.py:295-296`'s own precondition) and `is_authenticated`
    /// already re-checks expiry via a fresh clock read each time it is evaluated. This port keeps
    /// the same observable behaviour (a session that has expired is simply "not authenticated")
    /// rather than inventing a refresh call Python's own logic can never actually reach.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if [`AuthApi::is_authenticated`] is
    /// `false`.
    // `async` with no `.await` is deliberate: kept `async` for API parity with Python's
    // `async def ensure_authenticated()` and because a genuine refresh branch, while
    // unreachable today (see the doc comment above), would need to `.await` if it were ever
    // reachable in a future finding. Both async-related pedantic lints fire on this shape.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    pub async fn ensure_authenticated(&self) -> Result<(), Error> {
        if self.is_authenticated() {
            Ok(())
        } else {
            Err(Error::Authentication("Not authenticated".to_owned()))
        }
    }

    /// Seeds a session from a pre-obtained `token`, without any network call.
    ///
    /// Ported from `set_session_token()` (`api/auth.py:390-418`; brief lines 304-318): validates
    /// that `token` is non-empty (`auth.py:407-408`), builds a session with a fresh expiry of
    /// `now + `[`crate::consts::SESSION_LIFETIME_DAYS`]` days` (`auth.py:411-413`, the same rule
    /// [`Session::from_token`] applies), installs it as the current session, and persists it
    /// through the configured credential store — mirroring `_save_credentials()`
    /// (`auth.py:416`).
    ///
    /// Preserves any refresh token already held by the current session: Python's
    /// `set_session_token()` only ever assigns `session_id` and `session_expiry`
    /// (`auth.py:411-413`) and never touches `refresh_token` at all (brief lines 315-317), so
    /// whatever was already in memory survives untouched — the same preservation
    /// [`AuthApi::clear_session_token`] has to reproduce for the same reason. A session with no
    /// prior refresh token (the common case, since a normal login/verify never populates one)
    /// still ends up with `refresh_token: None`, matching [`Session::from_token`] directly.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "token", .. }` with Python's exact message
    /// (`"must be a non-empty string"`) if `token` is empty. Returns `Error::Storage` if the
    /// configured credential store failed to persist the new session — the in-memory session is
    /// installed regardless (see [`Transport::set_session`]'s docs).
    pub fn set_session_token(&self, token: &str) -> Result<(), Error> {
        if token.is_empty() {
            return Err(Error::Validation {
                field: "token".to_owned(),
                message: EMPTY_TOKEN_MESSAGE.to_owned(),
            });
        }
        let current = self.transport.session();
        let session =
            session_preserving_refresh_token(&Session::from_token(token), current.as_ref());
        self.transport.set_session(Some(session))
    }

    /// Clears only the session token and its expiry, leaving any refresh token in place.
    ///
    /// Ported from `clear_session_token()` (`api/auth.py:420-433`; brief lines 320-333): nulls
    /// `session_id` and `session_expiry` (`auth.py:427-428`) but deliberately does **not** touch
    /// `refresh_token` — a genuine difference from [`AuthApi::clear_auth_data`], which clears
    /// everything. The result is persisted through the credential store via a save (mirroring
    /// `_save_credentials()`, `auth.py:431`), not a store-entry removal.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if the configured credential store failed to persist the change —
    /// the in-memory session is updated regardless (see [`Transport::set_session`]'s docs).
    pub fn clear_session_token(&self) -> Result<(), Error> {
        let current = self.transport.session();
        let cleared = session_preserving_refresh_token(&Session::empty(), current.as_ref());
        self.transport.set_session(Some(cleared))
    }

    /// Clears the session token, refresh token, and expiry entirely, and removes the stored
    /// credential-store entry.
    ///
    /// Ported from `clear_auth_data()` (`api/auth.py:372-388`; brief lines 335-350):
    /// `clear_all()` nulls every field (`auth.py:379`), and this is the one auth-clearing method
    /// that calls the credential store's `clear()` rather than `save()` (`auth.py:386`) — for a
    /// file-backed or keyring-backed store this removes the entry entirely rather than
    /// overwriting it with null fields, in contrast with [`AuthApi::logout`] and
    /// [`AuthApi::clear_session_token`]. [`Transport::set_session`] with `None` implements exactly
    /// this: clears the in-memory session and, if a store is configured, calls
    /// `CredentialStore::clear`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if the configured credential store failed to clear its entry —
    /// the in-memory session is cleared regardless (see [`Transport::set_session`]'s docs).
    pub fn clear_auth_data(&self) -> Result<(), Error> {
        self.transport.set_session(None)
    }

    /// Clears the in-memory session to [`Session::empty`] and persists that clear via
    /// [`Transport::set_session`] on a blocking task — the underlying `CredentialStore` trait is
    /// synchronous (see `crate::storage`'s docs) — used only by [`AuthApi::logout`]'s
    /// unconditional cleanup.
    ///
    /// Honours this transport's configured [`StorageFailures`] policy (decision D-13, security
    /// finding T2): under the default [`StorageFailures::Warn`], a persistence failure —
    /// including the blocking task itself panicking or being cancelled — is logged at `WARN` and
    /// this returns `Ok(())`, matching [`Transport::set_session`]'s own contract exactly; under
    /// [`StorageFailures::Fatal`], the failure is returned as `Error::Storage` instead, so a
    /// caller who explicitly opted into "storage failures are fatal" can actually learn the
    /// at-rest copy was not overwritten. Either way, the in-memory session is cleared
    /// unconditionally and immediately — [`Transport::set_session`]'s own in-memory-first
    /// ordering guarantee (security finding T4) already covers that half; this method never
    /// weakens it.
    ///
    /// # Errors
    ///
    /// See above: only returns `Err` under [`StorageFailures::Fatal`].
    async fn clear_local_and_store(&self) -> Result<(), Error> {
        let transport = Arc::clone(&self.transport);
        match tokio::task::spawn_blocking(move || transport.set_session(Some(Session::empty())))
            .await
        {
            Ok(result) => result,
            Err(join_err) => {
                // Security finding T3: `JoinError`'s `Display` *and* `Debug` can carry a
                // panicking task's payload verbatim in the tokio version this crate pins
                // (1.53.1's `runtime::task::error` renders `task {id} panicked with message
                // {panic_str:?}` for both — this is not a "Display only" hazard).
                // `CredentialStore` is a public, pluggable trait, so a third-party backend that
                // panics (e.g. via `unwrap()`/`expect()`) on a value derived from the session it
                // was asked to persist could leak that text through this join error. Only
                // `is_panic()`/`is_cancelled()`/`id()` are ever read below — never `join_err`
                // itself, in any format.
                let message = format!(
                    "blocking session-clear task {} failed: panicked={}, cancelled={}",
                    join_err.id(),
                    join_err.is_panic(),
                    join_err.is_cancelled()
                );
                match self.transport.storage_failures() {
                    StorageFailures::Warn => {
                        tracing::warn!(
                            detail = %message,
                            "credential store task panicked or was cancelled"
                        );
                        Ok(())
                    }
                    StorageFailures::Fatal => Err(Error::Storage(StorageError::Backend {
                        backend: "credential-store-task".to_owned(),
                        message,
                    })),
                }
            }
        }
    }
}

/// Splices `current`'s refresh token (if any) into `base`, producing a session that combines
/// `base`'s token/expiry with `current`'s preserved refresh token.
///
/// Shared by [`AuthApi::set_session_token`] (`base` is a fresh [`Session::from_token`],
/// `auth.py:411-413` never touches `refresh_token`) and [`AuthApi::clear_session_token`] (`base`
/// is [`Session::empty`], matching `clear_session_token()`'s inline field assignments exactly,
/// `auth.py:427-428`: only `session_id` and `session_expiry` are reset) — both Python methods
/// leave `refresh_token` untouched, just with a different `base`.
///
/// Defers to [`Session::with_refresh_token`] (security finding T5), which builds the result
/// directly from one [`session::StoredSession`] to another and is infallible — this function used
/// to round-trip through [`Session::to_json`]/[`Session::from_json`] and a mutated
/// `serde_json::Value` instead, the same pattern `crate::transport::build_refreshed_session` used
/// to share, for the same underlying reason: [`session::StoredSession`]'s fields are private to
/// the `session` module, so a session with a *specific* combination of `base`'s fields and a
/// preserved refresh token cannot be assembled by touching private state from here.
fn session_preserving_refresh_token(base: &Session, current: Option<&Session>) -> Session {
    let refresh_token = current.and_then(Session::expose_refresh_token);
    Session::with_refresh_token(base, refresh_token)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use secrecy::ExposeSecret;

    use super::AuthApi;
    use crate::error::{Error, StorageError};
    use crate::storage::{CredentialStore, MemoryStore};
    use crate::transport::{StorageFailures, Transport};

    use super::Session;

    /// A session with a token, a refresh token, and an expiry far enough in the future that
    /// `is_valid()` is `true` for the lifetime of any test.
    fn session_with_refresh_token() -> Session {
        Session::from_json(
            r#"{"session_id":"tok","refresh_token":"rt-1","session_expiry":"2099-01-01T00:00:00"}"#,
        )
        .expect("valid json")
    }

    // ===================== set_session_token: empty-token validation =====================

    #[test]
    fn set_session_token_rejects_empty_token_with_pythons_message() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);

        let err = auth
            .set_session_token("")
            .expect_err("empty token must be rejected");
        assert!(matches!(
            err,
            Error::Validation { ref field, ref message }
                if field == "token" && message == "must be a non-empty string"
        ));
        assert_eq!(
            err.to_string(),
            "Validation error for 'token': must be a non-empty string"
        );
    }

    #[test]
    fn set_session_token_installs_a_valid_session() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);

        auth.set_session_token("tok-123")
            .expect("non-empty token is accepted");

        assert!(auth.is_authenticated());
        let session = auth.session().expect("session installed");
        assert_eq!(session.expose_token(), "tok-123");
        assert!(session.refresh_token().is_none());
    }

    #[test]
    fn set_session_token_preserves_an_existing_refresh_token() {
        // Finding 2 / Python parity (`auth.py:411-413`, brief lines 315-317):
        // `set_session_token()` only ever assigns `session_id` and `session_expiry`, never
        // `refresh_token` — so a refresh token already held by the current session must survive
        // the call, exactly like `clear_session_token`.
        let transport = Transport::builder()
            .session(Some(session_with_refresh_token()))
            .build()
            .expect("builds with an initial session");
        let auth = AuthApi::new(transport);

        auth.set_session_token("new-token")
            .expect("non-empty token is accepted");

        let session = auth.session().expect("session installed");
        assert_eq!(session.expose_token(), "new-token");
        assert_eq!(
            session
                .refresh_token()
                .expect("refresh token preserved")
                .expose_secret(),
            "rt-1"
        );
        assert!(session.is_valid());
    }

    // ===================== is_authenticated(): absent / valid / expired =====================

    #[test]
    fn is_authenticated_false_with_no_session_configured() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);
        assert!(!auth.is_authenticated());
        assert!(auth.session().is_none());
    }

    #[test]
    fn is_authenticated_true_with_a_valid_session() {
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok")))
            .build()
            .expect("builds with an initial session");
        let auth = AuthApi::new(transport);
        assert!(auth.is_authenticated());
    }

    #[test]
    fn is_authenticated_false_with_an_expired_session() {
        let expired = Session::from_json(
            r#"{"session_id":"tok","refresh_token":null,"session_expiry":"2000-01-01T00:00:00"}"#,
        )
        .expect("valid json");
        let transport = Transport::builder()
            .session(Some(expired))
            .build()
            .expect("builds with an initial session");
        let auth = AuthApi::new(transport);
        assert!(!auth.is_authenticated());
    }

    // ===================== the three clear_* scopes, against a MemoryStore =====================

    #[test]
    fn clear_session_token_nulls_token_but_preserves_refresh_token() {
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        let transport = Transport::builder()
            .session(Some(session_with_refresh_token()))
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with an initial session and a store");
        let auth = AuthApi::new(transport);

        auth.clear_session_token().expect("clears without error");

        assert!(!auth.is_authenticated());
        let session = auth
            .session()
            .expect("clear_session_token keeps a session value, just emptied");
        assert_eq!(session.expose_token(), "");
        assert_eq!(
            session
                .refresh_token()
                .expect("refresh token preserved in memory")
                .expose_secret(),
            "rt-1"
        );
        assert!(session.expiry().is_none());

        let persisted = store.load().expect("load succeeds");
        assert_eq!(persisted.expose_token(), "");
        assert_eq!(
            persisted
                .refresh_token()
                .expect("refresh token preserved in the store")
                .expose_secret(),
            "rt-1"
        );
    }

    #[test]
    fn clear_auth_data_clears_everything_and_removes_the_store_entry() {
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        let transport = Transport::builder()
            .session(Some(session_with_refresh_token()))
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with an initial session and a store");
        let auth = AuthApi::new(transport);

        auth.clear_auth_data().expect("clears without error");

        assert!(!auth.is_authenticated());
        assert!(
            auth.session().is_none(),
            "clear_auth_data clears the in-memory session to None"
        );

        let persisted = store.load().expect("load succeeds");
        assert_eq!(persisted.expose_token(), "");
        assert!(
            persisted.refresh_token().is_none(),
            "clear_auth_data clears the refresh token too, unlike clear_session_token"
        );
    }

    #[tokio::test]
    async fn logout_clears_local_state_and_store_even_when_not_authenticated() {
        // No valid session is configured, so `Transport::send`'s own precondition fires before
        // any network call — this exercises `logout`'s unconditional cleanup without needing a
        // mock server (HTTP-level logout tests are a later wave, per this crate's testing rules).
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        store
            .save(&session_with_refresh_token())
            .expect("seed the store with a stale entry");
        let transport = Transport::builder()
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with a store but no session");
        let auth = AuthApi::new(transport);

        let err = auth
            .logout()
            .await
            .expect_err("no valid session: the transport-level precondition fires locally");
        assert!(matches!(err, Error::Authentication(ref msg) if msg == "Not authenticated"));

        assert!(!auth.is_authenticated());
        let persisted = store.load().expect("load succeeds");
        assert_eq!(persisted.expose_token(), "");
        assert!(
            persisted.refresh_token().is_none(),
            "logout's cleanup clears the refresh token too (clear_all(), auth.py:270)"
        );
    }

    // ===================== logout under StorageFailures::Fatal (security finding T2) =====================

    /// A [`CredentialStore`] whose `save`/`clear` always fail, for pinning security finding T2:
    /// `logout()` must surface a `Fatal`-policy storage failure instead of silently returning the
    /// network `Ok` while the at-rest copy still holds the old session.
    #[derive(Debug)]
    struct AlwaysFailingStore;

    impl CredentialStore for AlwaysFailingStore {
        fn load(&self) -> Result<Session, StorageError> {
            Ok(Session::empty())
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            Err(StorageError::Backend {
                backend: "always-failing-test-store".to_owned(),
                message: "deliberate failure".to_owned(),
            })
        }

        fn clear(&self) -> Result<(), StorageError> {
            Err(StorageError::Backend {
                backend: "always-failing-test-store".to_owned(),
                message: "deliberate failure".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn logout_under_fatal_policy_surfaces_a_storage_failure_instead_of_hiding_it() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/2.2/logout"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let store: Arc<dyn CredentialStore> = Arc::new(AlwaysFailingStore);
        let transport = Transport::builder()
            .base_url(server.uri())
            .session(Some(session_with_refresh_token()))
            .store(Some(store))
            .storage_failures(StorageFailures::Fatal)
            .build()
            .expect("builds with a session, a failing store, and Fatal policy");
        let auth = AuthApi::new(transport);

        let err = auth.logout().await.expect_err(
            "Fatal policy must surface the store's failure even though the network call succeeded",
        );
        assert!(matches!(err, Error::Storage(_)));

        // The other half of finding T2: the in-memory session must still be cleared
        // unconditionally, exactly as under the default Warn policy.
        assert!(
            !auth.is_authenticated(),
            "in-memory session must be cleared even though persisting the clear failed"
        );
    }

    #[tokio::test]
    async fn logout_under_default_warn_policy_still_returns_the_network_outcome_when_storage_fails()
    {
        // No `.storage_failures(..)` call: exercises the default, `StorageFailures::Warn` — the
        // sibling of the `Fatal` test above, pinning that Warn's observable behaviour is
        // unchanged by the T2 fix.
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/2.2/logout"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let store: Arc<dyn CredentialStore> = Arc::new(AlwaysFailingStore);
        let transport = Transport::builder()
            .base_url(server.uri())
            .session(Some(session_with_refresh_token()))
            .store(Some(store))
            .build()
            .expect("builds with a session and a failing store");
        let auth = AuthApi::new(transport);

        auth.logout().await.expect(
            "the default Warn policy swallows the store failure and returns the network Ok",
        );

        assert!(
            !auth.is_authenticated(),
            "in-memory session must be cleared regardless of the store's outcome"
        );
    }

    // ===================== clear_local_and_store JoinError handling (security finding T3) =====================

    /// A [`CredentialStore`] whose `save`/`clear` panic with a distinctive, credential-shaped
    /// payload, isolating the `JoinError` branch of `AuthApi::clear_local_and_store` (security
    /// finding T3) from an ordinary `Err` returned by the store itself (`AlwaysFailingStore`,
    /// above).
    #[derive(Debug)]
    struct PanickingStore;

    impl CredentialStore for PanickingStore {
        fn load(&self) -> Result<Session, StorageError> {
            Ok(Session::empty())
        }

        fn save(&self, _session: &Session) -> Result<(), StorageError> {
            panic!("session_token=leaked-secret-should-never-appear-anywhere");
        }

        fn clear(&self) -> Result<(), StorageError> {
            panic!("session_token=leaked-secret-should-never-appear-anywhere");
        }
    }

    #[tokio::test]
    async fn clear_local_and_store_under_fatal_never_leaks_the_panic_payload_and_still_clears_memory()
     {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingStore);
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok-123")))
            .store(Some(store))
            .storage_failures(StorageFailures::Fatal)
            .build()
            .expect("builds with a panicking store");
        let auth = AuthApi::new(transport);

        let err = auth
            .clear_local_and_store()
            .await
            .expect_err("Fatal policy surfaces the panicked task as an error");
        assert!(matches!(err, Error::Storage(_)));

        let display = err.to_string();
        let debug = format!("{err:?}");
        assert!(
            !display.contains("leaked-secret") && !debug.contains("leaked-secret"),
            "the join error's own Display/Debug must never reach the returned error (T3): \
             display={display:?} debug={debug:?}"
        );

        assert!(
            !auth.is_authenticated(),
            "the in-memory session must be cleared even though the backend panicked"
        );
    }

    #[tokio::test]
    async fn clear_local_and_store_under_warn_swallows_the_panic_and_still_clears_memory() {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingStore);
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok-123")))
            .store(Some(store))
            .build()
            .expect("builds with a panicking store");
        let auth = AuthApi::new(transport);

        auth.clear_local_and_store()
            .await
            .expect("the default Warn policy swallows the panicked task and returns Ok");

        assert!(!auth.is_authenticated());
    }
}
