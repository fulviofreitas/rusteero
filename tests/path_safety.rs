//! Path-traversal regression suite for security review finding F1 (HIGH), plus one end-to-end
//! test for finding F3 (LOW).
//!
//! F1: an empty, `"."`, or `".."` path parameter must never reach the wire — `Url::path_segments_mut`
//! silently *drops* a segment equal to `""`, `"."`, or `".."` instead of encoding it
//! (`src/routes.rs`'s module docs, "Path traversal" section), which would otherwise retarget an
//! item-scoped destructive request onto its parent collection (`DELETE .../blacklist/{id}` with
//! `id = ".."` renders `DELETE .../blacklist`, unblocking every device on the network instead of
//! one). `src/routes.rs::validate_segment` and `src/transport.rs::Transport::render_url` share
//! one rule (`crate::routes::validate_segment`) so this cannot regress in only one of the two
//! renderers without failing here.
//!
//! Every destructive route below is exercised the same way, proving the point the module docs
//! make explicitly: checking only the returned error is not enough, because the bug was that a
//! request went out *at all*, to the wrong resource. Each `""`/`"."`/`".."` case therefore mounts
//! a catch-all `wiremock::matchers::any()` mock with `.expect(0)` — if `render_url`/`Route::render`
//! ever regress to the pre-fix behaviour, the mock server's own drop-time verification fails the
//! test, not just the returned `Result`. A positive case (an ordinary id) and a slash-bearing id
//! (proving percent-encoding still contains it in one path segment) are included per route too.
//!
//! F3: `Client::ensure_network_id`'s auto-discovery path and `Client::derive_preferred_network_id`
//! both extract a network id out of a `/networks` response body via the shared
//! `extract_network_id` helper (`src/client.rs`), which now applies the same
//! `crate::routes::validate_segment` rule before handing a candidate id back — see
//! `auto_discovered_network_id_rejects_a_hostile_id_field` below.

mod common;

use std::sync::Arc;

use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::endpoints::blacklist::BlacklistApi;
use rusteero::endpoints::forwards::ForwardsApi;
use rusteero::endpoints::profiles::ProfilesApi;
use rusteero::endpoints::reservations::ReservationsApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};

/// Mounts a catch-all mock on `mock` that fails the test (via `wiremock`'s own drop-time
/// verification) the moment *any* request, on any method or path, reaches the server — the
/// "zero requests reach the mock" assertion every hostile-id case below needs. Registered before
/// the call under test, exactly like the existing redirect-refusal guard in `tests/transport.rs`.
async fn expect_no_requests_at_all(mock: &MockEero) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
}

/// Asserts `err` is `Error::Validation` naming `field` exactly — the placeholder name from the
/// route template, not some other field — so a future change that swaps in a differently-named
/// validation error at the wrong call site still fails this assertion.
fn assert_validation_error_for_field(err: &Error, field: &str) {
    match err {
        Error::Validation { field: actual, .. } => {
            assert_eq!(actual, field, "unexpected field name on Error::Validation");
        }
        other => panic!("expected Error::Validation {{ field: {field:?}, .. }}, got {other:?}"),
    }
}

// ============================= remove_from_blacklist =============================

#[tokio::test]
async fn remove_from_blacklist_empty_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .remove_from_blacklist("network-0001", "")
        .await
        .expect_err("an empty id must not collapse DELETE .../blacklist/{id} onto the collection");
    assert_validation_error_for_field(&err, "mac_or_device_id");
    Ok(())
}

#[tokio::test]
async fn remove_from_blacklist_single_dot_id_is_rejected_before_any_request() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .remove_from_blacklist("network-0001", ".")
        .await
        .expect_err("a \".\" id must not collapse DELETE .../blacklist/{id} onto the collection");
    assert_validation_error_for_field(&err, "mac_or_device_id");
    Ok(())
}

/// The concrete reproduction from the security finding: before the fix, this DELETE landed on
/// `DELETE /2.2/networks/network-0001/blacklist` — the whole blacklist, unblocking every device
/// on the network — instead of erroring. Captured with `--nocapture` against the pre-fix
/// renderer (see this crate's final task report for the verbatim wrong URL); this must now be
/// rejected with zero requests sent.
#[tokio::test]
async fn remove_from_blacklist_dot_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .remove_from_blacklist("network-0001", "..")
        .await
        .expect_err("a \"..\" id must not collapse DELETE .../blacklist/{id} onto the collection");
    assert_validation_error_for_field(&err, "mac_or_device_id");
    Ok(())
}

#[tokio::test]
async fn remove_from_blacklist_normal_id_still_works() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/blacklist/aabbcc000001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.remove_from_blacklist("network-0001", "aabbcc000001")
        .await?;
    Ok(())
}

/// A slash-bearing id must still be percent-encoded into exactly one path segment, never split
/// into two — the pre-existing guard this fix must not weaken.
#[tokio::test]
async fn remove_from_blacklist_slash_bearing_id_stays_in_one_segment() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/blacklist/..%2F..%2Fsecrets",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = BlacklistApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.remove_from_blacklist("network-0001", "../../secrets")
        .await?;
    Ok(())
}

// ================================= delete_profile =================================

#[tokio::test]
async fn delete_profile_empty_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ProfilesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .delete_profile("network-0001", "")
        .await
        .expect_err("an empty id must not collapse DELETE .../profiles/{id} onto the collection");
    assert_validation_error_for_field(&err, "profile_id");
    Ok(())
}

#[tokio::test]
async fn delete_profile_single_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ProfilesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .delete_profile("network-0001", ".")
        .await
        .expect_err("a \".\" id must not collapse DELETE .../profiles/{id} onto the collection");
    assert_validation_error_for_field(&err, "profile_id");
    Ok(())
}

#[tokio::test]
async fn delete_profile_dot_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ProfilesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .delete_profile("network-0001", "..")
        .await
        .expect_err("a \"..\" id must delete one profile, never every profile on the network");
    assert_validation_error_for_field(&err, "profile_id");
    Ok(())
}

#[tokio::test]
async fn delete_profile_normal_id_still_works() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ProfilesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.delete_profile("network-0001", "profile-0001").await?;
    Ok(())
}

#[tokio::test]
async fn delete_profile_slash_bearing_id_stays_in_one_segment() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/profiles/..%2F..%2Fsecrets",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ProfilesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.delete_profile("network-0001", "../../secrets").await?;
    Ok(())
}

// ================================ delete_reservation ================================

#[tokio::test]
async fn delete_reservation_empty_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api.delete_reservation("network-0001", "").await.expect_err(
        "an empty id must not collapse DELETE .../reservations/{id} onto the collection",
    );
    assert_validation_error_for_field(&err, "reservation_id");
    Ok(())
}

#[tokio::test]
async fn delete_reservation_single_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .delete_reservation("network-0001", ".")
        .await
        .expect_err(
            "a \".\" id must not collapse DELETE .../reservations/{id} onto the collection",
        );
    assert_validation_error_for_field(&err, "reservation_id");
    Ok(())
}

#[tokio::test]
async fn delete_reservation_dot_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .delete_reservation("network-0001", "..")
        .await
        .expect_err(
            "a \"..\" id must delete one reservation, never every reservation on the network",
        );
    assert_validation_error_for_field(&err, "reservation_id");
    Ok(())
}

#[tokio::test]
async fn delete_reservation_normal_id_still_works() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/reservations/reservation-0001",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.delete_reservation("network-0001", "reservation-0001")
        .await?;
    Ok(())
}

#[tokio::test]
async fn delete_reservation_slash_bearing_id_stays_in_one_segment() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/reservations/..%2F..%2Fsecrets",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.delete_reservation("network-0001", "../../secrets")
        .await?;
    Ok(())
}

// ================================== delete_forward ==================================

#[tokio::test]
async fn delete_forward_empty_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ForwardsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .delete_forward("network-0001", "")
        .await
        .expect_err("an empty id must not collapse DELETE .../forwards/{id} onto the collection");
    assert_validation_error_for_field(&err, "forward_id");
    Ok(())
}

#[tokio::test]
async fn delete_forward_single_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ForwardsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .delete_forward("network-0001", ".")
        .await
        .expect_err("a \".\" id must not collapse DELETE .../forwards/{id} onto the collection");
    assert_validation_error_for_field(&err, "forward_id");
    Ok(())
}

#[tokio::test]
async fn delete_forward_dot_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ForwardsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .delete_forward("network-0001", "..")
        .await
        .expect_err("a \"..\" id must delete one forward, never every forward on the network");
    assert_validation_error_for_field(&err, "forward_id");
    Ok(())
}

#[tokio::test]
async fn delete_forward_normal_id_still_works() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/forwards/forward-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ForwardsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.delete_forward("network-0001", "forward-0001").await?;
    Ok(())
}

#[tokio::test]
async fn delete_forward_slash_bearing_id_stays_in_one_segment() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/forwards/..%2F..%2Fsecrets",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ForwardsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.delete_forward("network-0001", "../../secrets").await?;
    Ok(())
}

// ================================ update_reservation ================================
//
// `update_reservation` is a `PUT` carrying the caller's body — the finding's scariest variant,
// since a "delete everything" mistake is at least visibly destructive, but `PUT
// .../reservations` with `id = ".."` would silently apply the caller's single-reservation body
// to the *collection* endpoint instead.

#[tokio::test]
async fn update_reservation_empty_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .update_reservation("network-0001", "", json!({"ip": "10.0.0.5"}))
        .await
        .expect_err(
            "an empty id must not turn a single-reservation PUT into a collection-level one",
        );
    assert_validation_error_for_field(&err, "reservation_id");
    Ok(())
}

#[tokio::test]
async fn update_reservation_single_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .update_reservation("network-0001", ".", json!({"ip": "10.0.0.5"}))
        .await
        .expect_err(
            "a \".\" id must not turn a single-reservation PUT into a collection-level one",
        );
    assert_validation_error_for_field(&err, "reservation_id");
    Ok(())
}

#[tokio::test]
async fn update_reservation_dot_dot_id_is_rejected_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_requests_at_all(&mock).await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    let err = api
        .update_reservation("network-0001", "..", json!({"ip": "10.0.0.5"}))
        .await
        .expect_err(
            "a \"..\" id must not silently PUT the caller's body onto the reservations \
             collection endpoint",
        );
    assert_validation_error_for_field(&err, "reservation_id");
    Ok(())
}

#[tokio::test]
async fn update_reservation_normal_id_still_works() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/reservations/reservation-0001",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.update_reservation(
        "network-0001",
        "reservation-0001",
        json!({"ip": "10.0.0.5"}),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_reservation_slash_bearing_id_stays_in_one_segment() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/reservations/..%2F..%2Fsecrets",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = ReservationsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)));
    api.update_reservation("network-0001", "../../secrets", json!({"ip": "10.0.0.5"}))
        .await?;
    Ok(())
}

// ============================ F3: network-id auto-discovery ============================

/// Security review finding F3: a hostile or buggy `/networks` response must not hand
/// `Client::ensure_network_id`'s auto-discovery path a `".."` network id that then flows into
/// every subsequent request URL. Before the fix, `extract_network_id` accepted an `id` field of
/// `".."` verbatim; now it is rejected, auto-discovery finds no usable network, and the call
/// fails closed with `Error::MissingNetworkId` — the same outcome as an account with zero
/// networks — rather than resolving to a hostile id at all. The catch-all mock proves no request
/// is ever made to anything other than the `/networks` list itself (in particular, never to
/// `.../networks/..`).
#[tokio::test]
async fn auto_discovered_network_id_rejects_a_hostile_id_field() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(json!({"data": [{"id": ".."}]}).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    // Anything other than the `/networks` GET above — in particular a request derived from the
    // hostile `".."` id — must never be sent.
    Mock::given(wiremock::matchers::path_regex(r"^/2\.[23]/networks/.+$"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL");

    let err = client
        .get_network(None, false)
        .await
        .expect_err("a hostile \"..\" network id must not be usable for auto-discovery");
    assert!(
        matches!(err, Error::MissingNetworkId),
        "expected Error::MissingNetworkId, got {err:?}"
    );
    assert!(
        client.preferred_network_id().is_none(),
        "a rejected candidate must never be latched as the sticky preferred network id"
    );
    Ok(())
}
