//! HTTP integration tests for `ProfilesApi`'s read-only methods (`get_profiles`, `get_profile`,
//! `get_profile_devices`, `get_blocked_applications`), per `.claude/rules/testing.md`.
//!
//! The methods `get_profile`, `get_profile_devices` and `get_blocked_applications` all hit the
//! exact same wire endpoint (`GET networks/{network_id}/profiles/{profile_id}`) and must return
//! the exact same, untransformed envelope — the tests below pin both the path and the byte-for-
//! byte identical response for all three, which is what would catch a future change that
//! "helpfully" extracted a `devices` or `blocked_applications` sub-object for one of the aliases.

mod common;

use std::sync::Arc;

use rusteero::endpoints::profiles::ProfilesApi;
use rusteero::error::Error;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

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
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let env = api.get_profiles("network-0001").await?;

    assert_eq!(env.into_value(), fixture_json("profiles.json"));
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
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let env = api.get_profile("network-0001", "profile-0001").await?;

    assert_eq!(env.into_value(), fixture_json("profile.json"));
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
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let from_get_profile = api.get_profile("network-0001", "profile-0001").await?;
    let from_devices_alias = api
        .get_profile_devices("network-0001", "profile-0001")
        .await?;

    // Same wire call, same response, byte-identical envelope — neither a `devices` sub-object
    // nor anything else has been extracted or reshaped.
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

// ===================== get_blocked_applications: same call as get_profile =====================

#[tokio::test]
async fn get_blocked_applications_hits_the_same_path_as_get_profile_and_returns_an_identical_untransformed_envelope()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let from_get_profile = api.get_profile("network-0001", "profile-0001").await?;
    let from_blocked_apps_alias = api
        .get_blocked_applications("network-0001", "profile-0001")
        .await?;

    // Same wire call, same response, byte-identical envelope — neither a
    // `blocked_applications` sub-object nor anything else has been extracted or reshaped.
    assert_eq!(
        from_blocked_apps_alias.clone().into_value(),
        from_get_profile.into_value()
    );
    assert_eq!(
        from_blocked_apps_alias.into_value(),
        fixture_json("profile.json")
    );
    Ok(())
}

// ===================== 404 =====================

#[tokio::test]
async fn get_profile_404_maps_to_error_api_and_is_not_an_auth_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/missing-profile"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such profile"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let err = api
        .get_profile("network-0001", "missing-profile")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
