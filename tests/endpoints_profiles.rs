//! HTTP integration tests for `ProfilesApi` (v8.0.4) per the crate's testing conventions.
//!
//! `get_profile` and `get_profile_devices` hit the exact same wire endpoint (`GET
//! networks/{network_id}/profiles/{profile_id}`) and must return the exact same, untransformed
//! envelope — the tests below pin both the path and the byte-for-byte identical response for
//! both, which is what would catch a future change that "helpfully" extracted a `devices`
//! sub-object for the alias.

mod common;

use std::sync::Arc;

use rusteero::endpoints::profiles::ProfilesApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

/// Builds a [`ProfilesApi`] against `mock`, authenticated with [`TEST_TOKEN`].
fn profiles_api(mock: &MockEero) -> ProfilesApi {
    ProfilesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_profiles =====================

#[tokio::test]
async fn get_profiles_hits_v22_list_path_and_returns_the_fixture_envelope() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let env = api.get_profiles("network-0001", None).await?;

    assert_eq!(env.into_value(), fixture_json("profiles.json"));
    Ok(())
}

#[tokio::test]
async fn get_profiles_prefers_the_parents_published_profiles_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/profiles"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let parent = json!({"resources": {"profiles": "/2.4/networks/network-0001/profiles"}});
    api.get_profiles("network-0001", Some(&parent)).await?;
    Ok(())
}

// ===================== get_profile =====================

#[tokio::test]
async fn get_profile_hits_v22_single_profile_path_and_returns_the_fixture_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let env = api
        .get_profile("network-0001", "profile-0001", None)
        .await?;

    assert_eq!(env.into_value(), fixture_json("profile.json"));
    Ok(())
}

#[tokio::test]
async fn get_profile_prefers_the_parents_own_self_url_over_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-cached"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let parent = json!({"url": "/2.2/networks/network-0001/profiles/profile-cached"});
    // Bare `network_id`/`profile_id` below are ignored entirely once `parent`'s own `url`
    // resolves — matching Python's `self_url(resolved_parent) if ... else _profile_url(...)`.
    api.get_profile("ignored-network", "ignored-profile", Some(&parent))
        .await?;
    Ok(())
}

// ===================== get_profile_devices: same call as get_profile =====================

#[tokio::test]
async fn get_profile_devices_hits_the_same_path_as_get_profile_and_returns_an_identical_untransformed_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let from_get_profile = api
        .get_profile("network-0001", "profile-0001", None)
        .await?;
    let from_devices_alias = api
        .get_profile_devices("network-0001", "profile-0001", None)
        .await?;

    assert_eq!(
        from_devices_alias.clone().into_value(),
        from_get_profile.into_value()
    );
    assert_eq!(
        from_devices_alias.into_value(),
        fixture_json("profile.json")
    );
    Ok(())
}

// ===================== 404 =====================

#[tokio::test]
async fn get_profile_404_maps_to_error_not_found_and_is_not_an_auth_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/missing-profile"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such profile"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let err = api
        .get_profile("network-0001", "missing-profile", None)
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}

// ===================== pause_profile =====================

#[tokio::test]
async fn pause_profile_puts_paused_true_to_the_profile_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "paused": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let env = api
        .pause_profile("network-0001", "profile-0001", true, None)
        .await?;

    assert_eq!(env.meta().code, Some(200));
    Ok(())
}

#[tokio::test]
async fn pause_profile_puts_paused_false_to_unpause() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "paused": false })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.pause_profile("network-0001", "profile-0001", false, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn pause_profile_prefers_the_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001/profiles/profile-0001"});
    let api = profiles_api(&mock);
    api.pause_profile("network-0001", "profile-0001", true, Some(&parent))
        .await?;
    Ok(())
}

// ===================== set_profile_devices =====================

#[tokio::test]
async fn set_profile_devices_wraps_each_url_in_its_own_object_and_replaces_the_list()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "devices": [
                { "url": "/2.2/networks/network-0001/devices/device-0001" },
                { "url": "/2.2/networks/network-0001/devices/device-0002" }
            ]
        })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.set_profile_devices(
        "network-0001",
        "profile-0001",
        &[
            "/2.2/networks/network-0001/devices/device-0001",
            "/2.2/networks/network-0001/devices/device-0002",
        ],
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn set_profile_devices_prefers_the_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001/profiles/profile-0001"});
    let api = profiles_api(&mock);
    api.set_profile_devices(
        "network-0001",
        "profile-0001",
        &["/2.2/networks/network-0001/devices/device-0001"],
        Some(&parent),
    )
    .await?;
    Ok(())
}

// ===================== create_profile =====================

#[tokio::test]
async fn create_profile_posts_only_the_name_when_devices_and_paused_are_none() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "name": "Guests" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.create_profile("network-0001", "Guests", None, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn create_profile_includes_devices_and_paused_when_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .and(body_json(json!({
            "name": "Guests",
            "devices": [{ "url": "/2.2/networks/network-0001/devices/device-0001" }],
            "paused": false
        })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.create_profile(
        "network-0001",
        "Guests",
        Some(&["/2.2/networks/network-0001/devices/device-0001"]),
        Some(false),
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn create_profile_prefers_parent_profiles_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.4/networks/network-0001/profiles"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let parent = json!({"resources": {"profiles": "/2.4/networks/network-0001/profiles"}});
    api.create_profile("network-0001", "Guests", None, None, Some(&parent))
        .await?;
    Ok(())
}

// ===================== rename_profile =====================

#[tokio::test]
async fn rename_profile_puts_the_new_name() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "name": "Teenagers" })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.rename_profile("network-0001", "profile-0001", "Teenagers", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn rename_profile_prefers_the_parents_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001/profiles/profile-0001"});
    let api = profiles_api(&mock);
    api.rename_profile("network-0001", "profile-0001", "Teenagers", Some(&parent))
        .await?;
    Ok(())
}

// ===================== delete_profile =====================

#[tokio::test]
async fn delete_profile_sends_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.delete_profile("network-0001", "profile-0001").await?;
    Ok(())
}

#[tokio::test]
async fn delete_profile_404_maps_to_error_not_found_and_is_not_an_auth_error() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/profiles/missing-profile"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such profile"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let err = api
        .delete_profile("network-0001", "missing-profile")
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
