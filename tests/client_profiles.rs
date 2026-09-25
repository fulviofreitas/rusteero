//! `Client` integration suite for `ProfilesApi`'s pass-throughs, cache invalidation, and its
//! client-side content-filter validation.
//!
//! Every invalidation test asserts on the wiremock `.expect(n)` call count of the underlying
//! `GET`, never just the returned envelope — a caching test that only checks the value is not
//! testing caching at all (the crate's testing conventions' "Assertion Patterns").

mod common;

use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with the crate's
/// default 60-second cache TTL.
async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

// ===================== One representative mutation =====================

#[tokio::test]
async fn pause_profile_reaches_the_profile_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .pause_profile("profile-0001", true, Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("profile.json"));
    Ok(())
}

// ===================== Targeted invalidation: the core claim =====================

#[tokio::test]
async fn rename_profile_invalidates_the_profiles_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_profiles(Some("network-0001"), false).await?;
    client
        .rename_profile("profile-0001", "New Name", Some("network-0001"))
        .await?;
    client.get_profiles(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== Security review finding F4 =====================

/// **F4**: every caller-supplied key outside `VALID_CONTENT_FILTER_KEYS` is dropped client-side
/// before the request body is built (parity with Python). Before the fix, nothing guarded
/// against the resulting map being empty: a caller who only passed a misspelled key (e.g.
/// `block_adult_content` instead of `block_adult`) got a `200 OK` for `PUT {"content_filter":
/// {}}}`, which — if the server replaces rather than merges that nested object — silently clears
/// every content filter on the profile. This asserts the call now fails closed with
/// `Error::Validation` *before* any request is sent (`.expect(0)` on the PUT mock).
#[tokio::test]
async fn update_profile_content_filter_with_only_unknown_keys_is_a_validation_error()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .update_profile_content_filter(
            "profile-0001",
            &[("block_adult_content", true)],
            Some("network-0001"),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        rusteero::error::Error::Validation { ref field, .. } if field == "filters"
    ));
    Ok(())
}
