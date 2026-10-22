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
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// The network-facing half of `eero-api`'s `AuthAPI` (`src/eero/api/auth.py`): everything except
/// the interactive login handshake itself, which lives in [`flow`] as a separable type-state pair
/// ([`flow::LoginFlow`] / [`flow::PendingLogin`]).
///
/// Build one with [`AuthApi::new`], wrapping an already-configured [`Transport`] — the same
/// `Transport` a [`flow::LoginFlow`] can be pointed at, so a `Session` obtained from
/// [`flow::PendingLogin::verify`] and handed to `Transport::set_session` is immediately usable
/// here too. Use [`AuthApi::from_shared`] instead when the caller already holds an `Arc<Transport>`
/// it needs to keep sharing with other consumers (e.g. the `EeroApi` aggregator's domain
/// modules). `AuthApi` holds the `Transport` behind an `Arc` internally, so one `Transport` can
/// back an `AuthApi` and every domain module at once without requiring `Transport` itself to be
/// `Clone`.
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
    /// Ported from `logout()` (`api/auth.py:338-377` at `v8.4.1`). If no valid session is
    /// configured, logs a `WARNING` (`"Attempted to logout when not authenticated"`) and returns
    /// `Ok(false)` **with no network call at all** — unlike every other authenticated call in
    /// this crate, `logout` never raises `Error::Authentication("Not authenticated")` for this
    /// case, matching Python's own guard exactly. Otherwise, sends `POST` [`routes::LOGOUT`]
    /// authenticated by the current token, with a **form** body whose single field is literally
    /// named [`consts::LOGOUT_COOKIE_FIELD_NAME`] (`"Cookie"`) and whose value is
    /// [`consts::SESSION_COOKIE_PREFIX`] (`"s="`) followed by the token — not the real HTTP
    /// `Cookie` header, and not JSON (`auth.py:351-356`).
    ///
    /// Every failure mode from that request is ignored: `logout()` **never propagates a
    /// network/API error**. A `401` means the session is already invalid on the server and is
    /// logged at `DEBUG` (`"Session already invalid on server, clearing local credentials"`,
    /// `auth.py:359-361`); every other failure — a generic API error (which includes a `2xx`
    /// body that is not a JSON object), a network failure, a timeout — is logged at `WARN` as
    /// `"Logout API call failed: {err}"` (`auth.py:362-370`). A rate limit is swallowed the same
    /// way, where Python lets `EeroRateLimitException` propagate before it destroys anything.
    /// Credentials are always destroyed afterward, in memory and in every configured backend
    /// (`CredentialStore::clear`, not a `save` with an emptied session), regardless of the network
    /// outcome, and this then returns `Ok(true)`. The in-memory session is cleared first, in one
    /// synchronous step, and the store clear then runs on the runtime's blocking pool (see
    /// [`Transport::set_session_async`]), so every task that runs during that hop already sees
    /// the session gone (`auth.py:182-184` at `v8.4.1`).
    ///
    /// # Errors
    ///
    /// The only way this returns `Err` past the initial "not authenticated" case is a credential-
    /// store failure while clearing, under [`crate::transport::StorageFailures::Fatal`]: a
    /// caller who explicitly opted into "storage failures are fatal" can still learn the at-rest
    /// copy was not cleared. Under the default
    /// [`crate::transport::StorageFailures::Warn`], a storage failure here is logged at `WARN`
    /// and this still returns `Ok(true)`. The in-memory session is always cleared regardless of
    /// either outcome.
    pub async fn logout(&self) -> Result<bool, Error> {
        let Some(session) = self.transport.session().filter(Session::is_valid) else {
            tracing::warn!("Attempted to logout when not authenticated");
            return Ok(false);
        };
        let token = session.token().clone();

        let url = routes::LOGOUT.resolve(self.transport.api_host(), "", None)?;
        let cookie_value = format!(
            "{}{}",
            consts::SESSION_COOKIE_PREFIX,
            Session::expose_secret_token(&token)
        );
        let body = RequestBody::Form(vec![(
            consts::LOGOUT_COOKIE_FIELD_NAME.to_owned(),
            cookie_value,
        )]);

        match self
            .transport
            .request_with_token(Method::POST, url, &[], body, Some(&token))
            .await
        {
            Ok(_) => tracing::debug!("Logout API call succeeded"),
            // A 401 means the session is already invalid on the server (`auth.py:359-361`).
            Err(Error::Authentication { .. }) => {
                tracing::debug!("Session already invalid on server, clearing local credentials");
            }
            // Every other failure mode is swallowed too — logout never propagates a
            // network/API error. `err` is this crate's ordinary `Display`, which never embeds a
            // token or raw body text (see `crate::error::Error`'s own docs).
            Err(err) => tracing::warn!("Logout API call failed: {err}"),
        }

        // The in-memory session is cleared first, then the stored record is destroyed on the
        // blocking pool (`auth.py:167-184`, `_destroy_stored_credentials`).
        self.transport.set_session_async(None).await?;
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
    /// Ported from `set_session_token()` (`api/auth.py:560-585` at `v8.4.1`): validates that
    /// `token` is non-empty and printable ASCII with no CR/LF (`auth.py:578-580`, reusing
    /// `_validate_header_value`'s rule since the token becomes the literal `X-User-Token` header
    /// value), sets `session_id`, and persists through the configured credential store
    /// (`_save_credentials()`). `v8.0.4` fabricates no expiry of any kind (`AuthCredentials` has
    /// no such field) — a genuine simplification over the `v6.2.0` shape this method used to have.
    ///
    /// `async` since `eero-api` 8.1.10, where the store I/O moved to a worker thread so a slow
    /// keyring or disk no longer stalls the event loop: validation runs first and synchronously
    /// (its messages are unchanged), the in-memory session is installed before the store is
    /// touched, and the store write runs on the runtime's blocking pool (see
    /// [`Transport::set_session_async`]). When calls overlap, the last one applied wins in memory
    /// and in the store. [`AuthApi::credentials_persisted`] reports whether the store accepted
    /// the write.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "token", .. }` if `token` is empty or contains a byte
    /// outside the printable-ASCII range (or a CR/LF). Returns `Error::Storage` if the configured
    /// credential store failed to persist the new session under
    /// [`crate::transport::StorageFailures::Fatal`] — the in-memory session is installed
    /// regardless (see [`Transport::set_session_async`]'s docs).
    pub async fn set_session_token(&self, token: &str) -> Result<(), Error> {
        session::validate_token_shape("token", token)?;
        self.transport
            .set_session_async(Some(Session::from_token(token)))
            .await
    }

    /// Clears the current session token, in memory and in every configured credential-store
    /// backend.
    ///
    /// Ported from `clear_session_token()` (`api/auth.py:587-596` at `v8.4.1`): this does the
    /// same thing as [`AuthApi::clear_auth_data`] — `_destroy_stored_credentials()`, full
    /// `clear_all()` plus `storage.clear()` on every backend — since `AuthCredentials` is a
    /// one-field record (`session_id` only) with no `refresh_token`/`session_expiry` left to
    /// distinguish the two methods by. The one difference in Python, that `clear_auth_data` also
    /// resets `_login_in_progress`, has no counterpart here: a pending login lives in
    /// [`flow::PendingLogin`], never in the `AuthApi`, so there is no flag to reset. Kept as a
    /// separate method for API parity with Python.
    ///
    /// `async` since `eero-api` 8.1.10: the in-memory session is cleared first, then the store
    /// clear runs on the runtime's blocking pool (see [`Transport::set_session_async`]).
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if the configured credential store failed to clear its entry
    /// under [`crate::transport::StorageFailures::Fatal`] — the in-memory session is cleared
    /// regardless (see [`Transport::set_session_async`]'s docs).
    pub async fn clear_session_token(&self) -> Result<(), Error> {
        self.transport.set_session_async(None).await
    }

    /// Clears the current session entirely, in memory and in every configured credential-store
    /// backend.
    ///
    /// Ported from `clear_auth_data()` (`api/auth.py:549-558` at `v8.4.1`); see
    /// [`AuthApi::clear_session_token`]'s docs for why the two methods are identical here
    /// (Python's `clear_auth_data` also resets `_login_in_progress`, which has no counterpart on
    /// `AuthApi`) and for the `async` contract they share.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if the configured credential store failed to clear its entry
    /// under [`crate::transport::StorageFailures::Fatal`] — the in-memory session is cleared
    /// regardless (see [`Transport::set_session_async`]'s docs).
    pub async fn clear_auth_data(&self) -> Result<(), Error> {
        self.transport.set_session_async(None).await
    }

    /// Whether the most recent attempt to persist the session to the configured credential store
    /// succeeded.
    ///
    /// Ported from the `credentials_persisted` property (`api/auth.py:126-142` at `v8.4.1`):
    /// `true` until a save fails, and `true` again after a later successful save, a clear, or
    /// when no store is configured. Under the default
    /// [`crate::transport::StorageFailures::Warn`] policy a failed save is otherwise only a log
    /// line, and [`AuthApi::set_session_token`] still returns `Ok(())` with a session that works
    /// for this process but will not survive a restart; this is how a caller finds out. A thin
    /// wrapper over [`Transport::credentials_persisted`].
    #[must_use]
    pub fn credentials_persisted(&self) -> bool {
        self.transport.credentials_persisted()
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

    #[tokio::test]
    async fn set_session_token_rejects_empty_token_with_the_expected_message() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);

        let err = auth
            .set_session_token("")
            .await
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

    #[tokio::test]
    async fn set_session_token_rejects_a_control_character() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);

        let err = auth
            .set_session_token("tok\r\nX-Evil: 1")
            .await
            .expect_err("a control character must be rejected");
        assert!(matches!(err, Error::Validation { field, .. } if field == "token"));
    }

    #[tokio::test]
    async fn set_session_token_installs_a_valid_session() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let auth = AuthApi::new(transport);

        auth.set_session_token("tok-123")
            .await
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

    #[tokio::test]
    async fn clear_session_token_clears_everything_and_removes_the_store_entry() {
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok")))
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with an initial session and a store");
        let auth = AuthApi::new(transport);

        auth.clear_session_token()
            .await
            .expect("clears without error");

        assert!(!auth.is_authenticated());
        assert!(auth.session().is_none());
        let persisted = store.load().expect("load succeeds");
        assert!(persisted.token().expose_secret().is_empty());
    }

    #[tokio::test]
    async fn clear_auth_data_clears_everything_and_removes_the_store_entry() {
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok")))
            .store(Some(Arc::clone(&store)))
            .build()
            .expect("builds with an initial session and a store");
        let auth = AuthApi::new(transport);

        auth.clear_auth_data().await.expect("clears without error");

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

    // ===================== logout under StorageFailures::Fatal =====================

    /// A [`CredentialStore`] whose `save`/`clear` always fail, for pinning that `logout()` must
    /// surface a `Fatal`-policy storage failure instead of silently returning `Ok(true)` while
    /// the at-rest copy still holds the old session.
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

    // ===================== a store that panics on the blocking pool =====================

    /// A [`CredentialStore`] whose `save`/`clear` panic with a distinctive, credential-shaped
    /// payload, isolating the panicked-task branch of the blocking-pool persist (the panic
    /// payload must never reach a returned error) from an ordinary `Err` returned by the store
    /// itself.
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
    async fn clearing_under_fatal_never_leaks_the_panic_payload_and_still_clears_memory() {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingStore);
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok-123")))
            .store(Some(store))
            .storage_failures(StorageFailures::Fatal)
            .build()
            .expect("builds with a panicking store");
        let auth = AuthApi::new(transport);

        let err = auth
            .clear_auth_data()
            .await
            .expect_err("Fatal policy surfaces the panicked task as an error");
        assert!(matches!(err, Error::Storage(_)));

        let display = err.to_string();
        let debug = format!("{err:?}");
        assert!(
            !display.contains("leaked-secret") && !debug.contains("leaked-secret"),
            "the join error's own Display/Debug must never reach the returned error: \
             display={display:?} debug={debug:?}"
        );

        assert!(
            !auth.is_authenticated(),
            "the in-memory session must be cleared even though the backend panicked"
        );
    }

    #[tokio::test]
    async fn clearing_under_warn_swallows_the_panic_and_still_clears_memory() {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingStore);
        let transport = Transport::builder()
            .session(Some(Session::from_token("tok-123")))
            .store(Some(store))
            .build()
            .expect("builds with a panicking store");
        let auth = AuthApi::new(transport);

        auth.clear_auth_data()
            .await
            .expect("the default Warn policy swallows the panicked task and returns Ok");

        assert!(!auth.is_authenticated());
    }

    #[tokio::test]
    async fn a_panicking_save_reads_as_unpersisted_and_does_not_leak_its_payload() {
        let store: Arc<dyn CredentialStore> = Arc::new(PanickingStore);
        let transport = Transport::builder()
            .store(Some(store))
            .storage_failures(StorageFailures::Fatal)
            .build()
            .expect("builds with a panicking store");
        let auth = AuthApi::new(transport);

        let err = auth
            .set_session_token("tok-123")
            .await
            .expect_err("Fatal policy surfaces the panicked task as an error");
        assert!(matches!(err, Error::Storage(_)));
        let rendered = format!("{err} {err:?}");
        assert!(
            !rendered.contains("leaked-secret"),
            "payload leaked: {rendered}"
        );

        assert!(
            auth.is_authenticated(),
            "the session is installed in memory regardless"
        );
        assert!(
            !auth.credentials_persisted(),
            "a save that panicked did not persist"
        );

        // The gate the panicking store poisoned does not wedge later persists.
        auth.clear_session_token()
            .await
            .expect_err("the clear panics too under Fatal");
        assert!(auth.credentials_persisted(), "a clear resets the flag");
    }

    // ===================== logout: memory is cleared before the blocking hop =====================

    /// With the runtime's only blocking-pool thread occupied, the store clear that `logout`
    /// queues cannot start. The in-memory session must already be gone by then: every task that
    /// runs during the hop has to see the logged-out state, not the old token.
    #[test]
    fn logout_clears_memory_before_the_store_hop() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .expect("a current-thread runtime builds");

        runtime.block_on(async {
            // The mock server is an IP literal, so the request needs no blocking DNS lookup.
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .and(wiremock::matchers::path("/2.2/logout"))
                .and(wiremock::matchers::header("x-user-token", "tok"))
                .and(wiremock::matchers::header("cookie", "s=tok"))
                .and(wiremock::matchers::header(
                    "content-type",
                    "application/x-www-form-urlencoded",
                ))
                .and(wiremock::matchers::body_string("Cookie=s%3Dtok"))
                .respond_with(wiremock::ResponseTemplate::new(200))
                .expect(1)
                .mount(&server)
                .await;

            let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
            store
                .save(&Session::from_token("tok"))
                .expect("seed the store");
            let transport = Transport::builder()
                .base_url(server.uri())
                .session(Some(Session::from_token("tok")))
                .store(Some(Arc::clone(&store)))
                .build()
                .expect("builds with a session and a store");
            let auth = Arc::new(AuthApi::new(transport));

            // Occupy the pool's only thread until released.
            let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
            let (entered_tx, entered_rx) = tokio::sync::oneshot::channel::<()>();
            let blocker = tokio::task::spawn_blocking(move || {
                let _ = entered_tx.send(());
                let _ = release_rx.recv();
            });
            entered_rx.await.expect("the blocker holds the only thread");

            let logout = tokio::spawn({
                let auth = Arc::clone(&auth);
                async move { auth.logout().await }
            });

            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while auth.is_authenticated() {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("the session is cleared although the blocking pool is saturated");
            assert!(
                !logout.is_finished(),
                "logout is still waiting for the blocking pool, so memory was cleared first"
            );
            assert_eq!(
                server
                    .received_requests()
                    .await
                    .expect("request recording is on")
                    .len(),
                1,
                "the logout request itself was sent"
            );
            assert_eq!(
                store.load().expect("load").token().expose_secret(),
                "tok",
                "the store clear has not run yet"
            );

            release_tx.send(()).expect("the blocker is still waiting");
            blocker.await.expect("the blocker finishes");
            assert!(
                logout
                    .await
                    .expect("the logout task completes")
                    .expect("logout succeeds"),
                "logout reports success"
            );
            assert!(
                store
                    .load()
                    .expect("load")
                    .token()
                    .expose_secret()
                    .is_empty()
            );
        });
    }
}
