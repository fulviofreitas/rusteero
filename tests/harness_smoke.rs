//! Self-test for the shared wiremock harness (`tests/common/mod.rs`).
//!
//! This is not a test of `rusteero` itself — every behaviour it exercises is already covered by
//! `src/transport.rs`'s own unit tests — it exists purely to prove `MockEero` actually reaches a
//! running mock server for both API versions before any other test file starts depending on it.

mod common;

use reqwest::Method;
use rusteero::error::Error;
use rusteero::routes::{ACCOUNT, ApiVersion, Nested};
use rusteero::transport::RequestBody;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

// ===================== transport_with_token: reaches the mock on v2.2 =====================

#[tokio::test]
async fn transport_with_token_reaches_the_mock_server_on_v2_2() {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect("mock server responds 200 with a valid envelope");

    assert_eq!(env.meta().code, Some(200));
    assert_eq!(env.data()["id"], json!("user-0001"));
}

// ===================== base_url override: both API versions land on the mock =====================

#[tokio::test]
async fn base_url_override_points_both_api_versions_at_the_mock_server() {
    let mock = MockEero::start().await;

    // A `Nested` for a device PUT, assembled ad hoc exactly as `routes/mod.rs`'s own doc comment
    // permits, purely to prove the harness's base-URL wiring, not to test device semantics.
    let device_put = Nested {
        method: Method::PUT,
        version: ApiVersion::V2_3,
        prefix: "devices",
        suffix: "",
        link: None,
    };

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = mock.transport_with_token(TEST_TOKEN);
    let env = transport
        .nested(
            &device_put,
            "network-0001",
            "aa:bb",
            None,
            &[],
            RequestBody::Json(json!({ "nickname": "renamed" })),
        )
        .await
        .expect("mock server accepts the v2.3 PUT");

    assert_eq!(env.into_value(), json!({}));
}

// ===================== transport_anonymous: no session, never touches the network =====================

#[tokio::test]
async fn transport_anonymous_has_no_session_and_never_calls_the_network() {
    let mock = MockEero::start().await;
    // Deliberately no `Mock` registered: if this ever reached the network, wiremock would
    // respond 404 rather than this test's expected `Error::Authentication`, and there is no
    // `.expect(n)` to violate either way — the absence of a registered mock is itself the
    // assertion that no request was ever sent.
    let transport = mock.transport_anonymous();

    let err = transport
        .resource(&ACCOUNT, "", None, &[], RequestBody::None)
        .await
        .expect_err("no session configured means the precondition fires before any request");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Not authenticated")
    );
}

// ===================== login_flow: hits the mock's /login route =====================

#[tokio::test]
async fn login_flow_from_mock_eero_hits_the_login_route() {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let flow = mock.login_flow();
    // `PendingLogin` redacts its token in `Debug`, so this is safe to construct and drop without
    // asserting on its (private) contents from an external test crate.
    let _pending = flow
        .start("user@example.com")
        .await
        .expect("mock server returns a non-empty user_token");
}

// ===================== auth_api_with_token: immediately authenticated, no network call =====================

#[tokio::test]
async fn auth_api_with_token_is_immediately_authenticated() {
    let mock = MockEero::start().await;
    let auth = mock.auth_api_with_token(TEST_TOKEN);
    assert!(auth.is_authenticated());
    assert!(auth.session().is_some());
}

// ===================== fixture / fixture_json =====================

#[test]
fn every_minimum_fixture_parses_as_an_envelope_shaped_object() {
    for name in [
        "login.json",
        "verify.json",
        "account.json",
        "networks.json",
        "network.json",
        "devices.json",
        "device.json",
        "eeros.json",
        "eero.json",
        "profiles.json",
        "profile.json",
    ] {
        let value = fixture_json(name);
        assert!(value.get("meta").is_some(), "{name} is missing `meta`");
        assert!(value.get("data").is_some(), "{name} is missing `data`");
    }
}

#[test]
fn network_and_device_ids_are_consistent_across_fixtures() {
    // Ids must be shared across fixtures so tests can
    // compose them (e.g. fetch `networks.json`, then `network.json` for one of its ids).
    let network_id = fixture_json("network.json")["data"]["id"].clone();
    assert_eq!(fixture_json("networks.json")["data"][0]["id"], network_id);
    assert_eq!(
        fixture_json("account.json")["data"]["networks"]["data"][0]["id"],
        network_id
    );

    let device_url = fixture_json("device.json")["data"]["url"].clone();
    assert_eq!(fixture_json("devices.json")["data"][0]["url"], device_url);
    assert_eq!(
        fixture_json("profile.json")["data"]["devices"][0]["url"],
        device_url
    );
}
