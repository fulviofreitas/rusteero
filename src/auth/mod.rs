//! Authentication: the session credential value ([`Session`]), its on-disk storage
//! representation, the interactive login handshake ([`flow`]), and the network-facing
//! authentication API ([`AuthApi`]).
//!
//! Ported from `eero-api`'s `src/eero/api/auth.py` at `v8.0.4`.
pub mod flow;
pub mod session;

pub use session::Session;

use std::sync::Arc;

use reqwest::Method;

use crate::consts;
use crate::error::{Error, StorageError};
use crate::routes;
use crate::transport::{RequestBody, StorageFailures, Transport};

/// The network-facing half of `eero-api`'s `AuthAPI` (`src/eero/api/auth.py`): everything except
/// the interactive login handshake itself, which lives in [`flow`] as a separable type-state pair
/// ([`flow::LoginFlow`] / [`flow::PendingLogin`]).
///
/// Build one with [`AuthApi::new`], wrapping an already-configured [`Transport`] — the same
/// `Transport` a [`flow::LoginFlow`] can be pointed at, so a `Session` obtained from
/// [`flow::PendingLogin::verify`] and handed to `Transport::set_session` is immediately usable
/// here too. Use [`AuthApi::from_shared`] instead when the caller already holds an `Arc<Transport>`
/// it needs to keep sharing with other consumers (e.g. the `EeroApi` aggregator's domain
/// modules). `AuthApi` holds the `Transport` behind an `Arc` internally so that [`AuthApi::logout`]
/// can move a handle onto a `tokio::task::spawn_blocking` task for its credential-store write
/// without requiring `Transport` itself to be `Clone`.
#[derive(Debug)]
pub struct AuthApi {
    transport: Arc<Transport>,
}

impl AuthApi {
    /// Wraps `transport` as an `AuthApi`, allocating a new `Arc` around it.
    #[must_use]
    pub fn new(transport: Transport) -> Self {
        Self::from_shared(Arc::new(transport))
    }

    /// Wraps an already-shared `transport` as an `AuthApi`, without allocating a new `Arc`.
    #[must_use]
    pub fn from_shared(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Borrows the underlying [`Transport`], for callers (e.g. the `EeroApi` aggregator) that
    /// need to issue their own requests through the same transport this `AuthApi` uses.
    #[must_use]
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Whether a session is configured and has a non-empty token.
    ///
    /// Ported from the `is_authenticated` property (`api/auth.py:108-118`): a pure token-presence
    /// check — `v8.0.4` has no client-side session expiry anywhere ("There is no client-side
    /// session expiry -- the server is the sole authority on session validity, signalled via 401
    /// responses.") — delegating entirely to [`Transport::is_authenticated`]. Never makes a
    /// network call and never attempts a refresh.
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
    /// Ported from `logout()` (`api/auth.py:293-330`). If no valid session is configured, logs a
    /// `WARNING` (`"Attempted to logout when not authenticated"`) and returns `Ok(false)` **with
    /// no network call at all** (`auth.py:307-309`) — unlike every other authenticated call in
    /// this crate, `logout` never raises `Error::Authentication("Not authenticated")` for this
    /// case, matching Python's own guard exactly. Otherwise, sends `POST` [`routes::LOGOUT`]
    /// authenticated by the current token, with a **form** body whose single field is literally
    /// named [`consts::LOGOUT_COOKIE_FIELD_NAME`] (`"Cookie"`) and whose value is
    /// [`consts::SESSION_COOKIE_PREFIX`] (`"s="`) followed by the token — not the real HTTP
    /// `Cookie` header, and not JSON (`auth.py:309-314`).
    ///
    /// Every failure mode from that request — an authentication error, a generic API error, a
    /// rate limit, a network failure, a timeout — is logged at `WARN` and otherwise ignored
    /// (`auth.py:315-327`): `logout()` **never propagates a network/API error**. Credentials are
    /// always destroyed afterward, in memory and in every configured backend
    /// (`CredentialStore::clear`, not a `save` with an emptied session), regardless of the network
    /// outcome, and this then returns `Ok(true)`.
    ///
    /// # Errors
    ///
    /// The only way this returns `Err` past the initial "not authenticated" case is a credential-
    /// store failure while clearing, under [`crate::transport::StorageFailures::Fatal`] (decision
    /// D-13, security finding T2): a caller who explicitly opted into "storage failures are
    /// fatal" can still learn the at-rest copy was not cleared. Under the default
    /// [`crate::transport::StorageFailures::Warn`], a storage failure here is logged at `WARN`
    /// and this still returns `Ok(true)`. The in-memory session is always cleared regardless of
    /// either outcome.
    pub async fn logout(&self) -> Result<bool, Error> {
        let Some(session) = self.transport.session().filter(Session::is_valid) else {
            tracing::warn!("Attempted to logout when not authenticated");
            return Ok(false);
        };
        let token = session.token().clone();

        let url = self.transport.render_url(&routes::LOGOUT, &[])?;
        let cookie_value = format!(
            "{}{}",
            consts::SESSION_COOKIE_PREFIX,
            Session::expose_secret_token(&token)
        );
        let body = RequestBody::Form(vec![(
            consts::LOGOUT_COOKIE_FIELD_NAME.to_owned(),
            cookie_value,
        )]);

        if let Err(err) = self
            .transport
            .request_with_token(Method::POST, url, &[], body, Some(&token))
            .await
        {
            // Every failure mode is swallowed here, per `auth.py:315-327` — logout never
            // propagates a network/API error. `error` is this crate's ordinary `Display`, which
            // never embeds a token or raw body text (see `crate::error::Error`'s own docs).
            tracing::warn!(error = %err, "logout request failed; credentials are still cleared locally");
        }

        self.clear_local_and_store().await?;
        Ok(true)
    }

    /// Attempts to refresh the current session.
    ///
    /// A thin wrapper over [`Transport::refresh_session`]; see that method's docs for the full
    /// behaviour, ported from `refresh_session()`/`_do_refresh()` (`api/auth.py:331-477`).
    ///
    /// # Errors
    ///
    /// See [`Transport::refresh_session`].
    pub async fn refresh_session(&self) -> Result<bool, Error> {
        self.transport.refresh_session().await
    }

    /// Checks whether the current session is usable, without attempting a network refresh.
    ///
    /// Ported from `ensure_authenticated()` (`api/auth.py:479-489`): a bare alias for
    /// [`AuthApi::is_authenticated`] — `v8.0.4` has no expiry-driven refresh branch of any kind
    /// (there is no client-side expiry concept left to trigger one, see
    /// [`AuthApi::is_authenticated`]'s own docs).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if [`AuthApi::is_authenticated`] is
    /// `false`.
    // `async` with no `.await` is deliberate: kept `async` for API parity with Python's
    // `async def ensure_authenticated()`.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    pub async fn ensure_authenticated(&self) -> Result<(), Error> {
        if self.is_authenticated() {
            Ok(())
        } else {
            Err(Error::authentication("Not authenticated"))
        }
    }

    /// Seeds a session from a pre-obtained `token`, without any network call.
    ///
    /// Ported from `set_session_token()` (`api/auth.py:511-537`): validates that `token` is
    /// non-empty and printable ASCII with no CR/LF (`auth.py:530-532`, reusing
    /// `_validate_header_value`'s rule since the token becomes the literal `X-User-Token` header
    /// value), sets `session_id`, and persists through the configured credential store
    /// (`_save_credentials()`). `v8.0.4` fabricates no expiry of any kind (`AuthCredentials` has
    /// no such field) — a genuine simplification over the `v6.2.0` shape this method used to have.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "token", .. }` if `token` is empty or contains a byte
    /// outside the printable-ASCII range (or a CR/LF). Returns `Error::Storage` if the configured
    /// credential store failed to persist the new session — the in-memory session is installed
    /// regardless (see [`Transport::set_session`]'s docs).
    pub fn set_session_token(&self, token: &str) -> Result<(), Error> {
        session::validate_token_shape("token", token)?;
        self.transport.set_session(Some(Session::from_token(token)))
    }

    /// Clears the current session token, in memory and in every configured credential-store
    /// backend.
    ///
    /// Ported from `clear_session_token()` (`api/auth.py:501-510`): at `v8.0.4` this does the
    /// **exact same thing** as [`AuthApi::clear_auth_data`] — `_destroy_stored_credentials()`,
    /// full `clear_all()` plus `storage.clear()` on every backend — since `AuthCredentials` is a
    /// one-field record (`session_id` only) with no `refresh_token`/`session_expiry` left to
    /// distinguish the two methods by. Kept as a separate method for API parity with Python.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if the configured credential store failed to clear its entry —
    /// the in-memory session is cleared regardless (see [`Transport::set_session`]'s docs).
    pub fn clear_session_token(&self) -> Result<(), Error> {
        self.transport.set_session(None)
    }

    /// Clears the current session entirely, in memory and in every configured credential-store
    /// backend.
    ///
    /// Ported from `clear_auth_data()` (`api/auth.py:539-547`); see
    /// [`AuthApi::clear_session_token`]'s docs for why the two methods are identical at `v8.0.4`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if the configured credential store failed to clear its entry —
    /// the in-memory session is cleared regardless (see [`Transport::set_session`]'s docs).
    pub fn clear_auth_data(&self) -> Result<(), Error> {
        self.transport.set_session(None)
    }

    /// Clears the in-memory session to `None` and persists that clear via [`Transport::set_session`]
    /// on a blocking task — the underlying `CredentialStore` trait is synchronous — used only by
    /// [`AuthApi::logout`]'s unconditional cleanup.
    ///
    /// Honours this transport's configured [`StorageFailures`] policy: under the default
    /// [`StorageFailures::Warn`], a persistence failure — including the blocking task itself
    /// panicking or being cancelled — is logged at `WARN` and this returns `Ok(())`; under
    /// [`StorageFailures::Fatal`], the failure is returned as `Error::Storage` instead. Either
    /// way, the in-memory session is cleared unconditionally and immediately.
    ///
    /// # Errors
    ///
    /// See above: only returns `Err` under [`StorageFailures::Fatal`].
    async fn clear_local_and_store(&self) -> Result<(), Error> {
        let transport = Arc::clone(&self.transport);
        match tokio::task::spawn_blocking(move || transport.set_session(None)).await {
            Ok(result) => result,
            Err(join_err) => {
                // Security finding T3: `JoinError`'s `Display`/`Debug` can carry a panicking
                // task's payload verbatim. `CredentialStore` is a public, pluggable trait, so a
                // third-party backend that panics on a value derived from the session it was
                // asked to persist could leak that text through this join error. Only
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use secrecy::ExposeSecret;

    use super::AuthApi;
    use crate::error::{Error, StorageError};
    use crate::storage::{CredentialStore, MemoryStore};
    use crate::transport::{StorageFailures, Transport};

    use super::Session;

    // ===================== set_session_token: validation =====================

    #[test]
    fn set_session_token_rejects_empty_token_with_the_expected_message() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);

        let err = auth
            .set_session_token("")
            .expect_err("empty token must be rejected");
        assert!(matches!(
            err,
            Error::Validation { ref field, ref message, .. }
                if field == "token" && message == "must be a non-empty string"
        ));
        assert_eq!(
            err.to_string(),
            "Validation error for 'token': must be a non-empty string"
        );
    }

    #[test]
    fn set_session_token_rejects_a_control_character() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);

        let err = auth
            .set_session_token("tok\r\nX-Evil: 1")
            .expect_err("a control character must be rejected");
        assert!(matches!(err, Error::Validation { field, .. } if field == "token"));
    }

    #[test]
    fn set_session_token_installs_a_valid_session() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);

        auth.set_session_token("tok-123")
            .expect("non-empty token is accepted");

        assert!(auth.is_authenticated());
        let session = auth.session().expect("session installed");
        assert_eq!(session.token().expose_secret(), "tok-123");
    }

    // ===================== is_authenticated(): absent / valid =====================

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

    // ===================== the two clear_* scopes are now identical =====================

    #[test]
    fn clear_session_token_clears_everything_and_removes_the_store_entry() {
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok")))
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with an initial session and a store");
        let auth = AuthApi::new(transport);

        auth.clear_session_token().expect("clears without error");

        assert!(!auth.is_authenticated());
        assert!(auth.session().is_none());
        let persisted = store.load().expect("load succeeds");
        assert!(persisted.token().expose_secret().is_empty());
    }

    #[test]
    fn clear_auth_data_clears_everything_and_removes_the_store_entry() {
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok")))
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with an initial session and a store");
        let auth = AuthApi::new(transport);

        auth.clear_auth_data().expect("clears without error");

        assert!(!auth.is_authenticated());
        assert!(auth.session().is_none());
        let persisted = store.load().expect("load succeeds");
        assert!(persisted.token().expose_secret().is_empty());
    }

    // ===================== logout: not authenticated, no network call =====================

    #[tokio::test]
    async fn logout_when_not_authenticated_returns_false_with_no_network_call() {
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        store
            .save(&Session::from_token("stale-token"))
            .expect("seed the store with a stale entry");
        let transport = Transport::builder()
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with a store but no session");
        let auth = AuthApi::new(transport);

        // No mock server is configured at all — if `logout` ever attempted a network call, it
        // would panic on connection refused rather than return `Ok(false)`.
        let result = auth
            .logout()
            .await
            .expect("no session configured: the local guard fires, no network call attempted");
        assert!(!result);

        // The not-authenticated guard does not touch the store at all — the stale entry from
        // before this call is still exactly what it was.
        let persisted = store.load().expect("load succeeds");
        assert_eq!(persisted.token().expose_secret(), "stale-token");
    }

    // ===================== logout under StorageFailures::Fatal (security finding T2) =====================

    /// A [`CredentialStore`] whose `save`/`clear` always fail, for pinning security finding T2:
    /// `logout()` must surface a `Fatal`-policy storage failure instead of silently returning
    /// `Ok(true)` while the at-rest copy still holds the old session.
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
            .session(Some(Session::from_token("tok")))
            .store(Some(store))
            .storage_failures(StorageFailures::Fatal)
            .build()
            .expect("builds with a session, a failing store, and Fatal policy");
        let auth = AuthApi::new(transport);

        let err = auth.logout().await.expect_err(
            "Fatal policy must surface the store's failure even though the network call succeeded",
        );
        assert!(matches!(err, Error::Storage(_)));
        assert!(
            !auth.is_authenticated(),
            "in-memory session must be cleared even though persisting the clear failed"
        );
    }

    #[tokio::test]
    async fn logout_under_default_warn_policy_still_returns_true_when_storage_fails() {
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
            .session(Some(Session::from_token("tok")))
            .store(Some(store))
            .build()
            .expect("builds with a session and a failing store");
        let auth = AuthApi::new(transport);

        let result = auth
            .logout()
            .await
            .expect("the default Warn policy swallows the store failure and returns Ok(true)");
        assert!(result);
        assert!(!auth.is_authenticated());
    }

    #[tokio::test]
    async fn logout_swallows_every_network_failure_and_still_clears_credentials() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/2.2/logout"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .expect(1)
            .mount(&server)
            .await;

        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        let transport = Transport::builder()
            .base_url(server.uri())
            .session(Some(Session::from_token("tok")))
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with a session and a working store");
        let auth = AuthApi::new(transport);

        let result = auth
            .logout()
            .await
            .expect("a 500 from the logout endpoint must never propagate");
        assert!(result);
        assert!(!auth.is_authenticated());
        assert!(
            store
                .load()
                .expect("load succeeds")
                .token()
                .expose_secret()
                .is_empty()
        );
    }

    // ===================== clear_local_and_store JoinError handling (security finding T3) =====================

    /// A [`CredentialStore`] whose `save`/`clear` panic with a distinctive, credential-shaped
    /// payload, isolating the `JoinError` branch of `AuthApi::clear_local_and_store` (security
    /// finding T3) from an ordinary `Err` returned by the store itself.
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
