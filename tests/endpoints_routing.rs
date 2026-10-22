//! HTTP integration tests for `RoutingApi` (`src/endpoints/routing.rs`).
//!
//! `RoutingApi` has no fixture under `tests/fixtures/`, so the response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_routing.py`). No real MACs, serials, IPs or names appear in any fixture
//! here.

mod common;

use std::sync::Arc;

use rusteero::endpoints::routing::RoutingApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn routing_api(mock: &MockEero) -> RoutingApi {
    RoutingApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Every spelling of the network `get_routing` must pin to 2.2: a bare id, API paths and absolute
/// API-host URLs on either version, and absolute URLs whose scheme is not lower case.
const NETWORK_FORMS: [&str; 7] = [
    "id",
    "path-2.2",
    "path-2.3",
    "url-2.2",
    "url-2.3",
    "upper-case-scheme-url-2.3",
    "mixed-case-scheme-url-2.3",
];

/// Renders one of [`NETWORK_FORMS`] against the mock server's origin.
fn network_form(uri: &str, form: &str) -> String {
    let upper = uri.replacen("http", "HTTP", 1);
    let mixed = uri.replacen("http", "hTTp", 1);
    match form {
        "id" => "network-0001".to_owned(),
        "path-2.2" => "/2.2/networks/network-0001".to_owned(),
        "path-2.3" => "/2.3/networks/network-0001".to_owned(),
        "url-2.2" => format!("{uri}/2.2/networks/network-0001"),
        "url-2.3" => format!("{uri}/2.3/networks/network-0001"),
        "upper-case-scheme-url-2.3" => format!("{upper}/2.3/networks/network-0001"),
        "mixed-case-scheme-url-2.3" => format!("{mixed}/2.3/networks/network-0001"),
        other => panic!("unknown network form {other}"),
    }
}

/// Mounts a catch-all mock that fails the test if it receives any request at all.
async fn expect_no_request(mock: &MockEero) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
}

// ===================== get_routing =====================

#[tokio::test]
async fn get_routing_bare_id_builds_the_default_v22_template_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"routes": [], "mode": "automatic"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/routing"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = routing_api(&mock).get_routing("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_routing_prefers_the_parents_routing_link_over_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"routes": []} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/routing"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/custom-routing"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"routing": "/2.2/networks/network-0001/custom-routing"}});
    let env = routing_api(&mock)
        .get_routing("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_routing_pins_to_2_2_even_with_2_3_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {} });
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/routing"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/routing"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({
        "url": "/2.2/networks/network-0001",
        "resources": {"routing": "/2.3/networks/network-0001/routing"},
    });
    let env = routing_api(&mock)
        .get_routing("network-0001", Some(&parent))
        .await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_routing_pins_a_published_link_on_any_version_to_2_2() -> anyhow::Result<()> {
    for version in ["2.2", "2.3", "2.4", "10.25"] {
        let mock = MockEero::start().await;
        Mock::given(method("GET"))
            .and(path("/2.2/networks/network-0001/routing"))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&mock.server)
            .await;

        let link = format!("/{version}/networks/network-0001/routing");
        let parent = json!({"resources": {"routing": link}});
        routing_api(&mock)
            .get_routing("network-0001", Some(&parent))
            .await
            .unwrap_or_else(|err| panic!("published link on {version}: {err}"));
    }
    Ok(())
}

#[tokio::test]
async fn get_routing_pins_to_2_2_for_every_network_form() -> anyhow::Result<()> {
    for form in NETWORK_FORMS {
        let mock = MockEero::start().await;
        Mock::given(method("GET"))
            .and(path("/2.3/networks/network-0001/routing"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock.server)
            .await;
        Mock::given(method("GET"))
            .and(path("/2.2/networks/network-0001/routing"))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&mock.server)
            .await;

        let network = network_form(&mock.uri(), form);
        routing_api(&mock)
            .get_routing(&network, None)
            .await
            .unwrap_or_else(|err| panic!("network form {form}: {err}"));
    }
    Ok(())
}

/// A whitespace or control character in a caller's network path or URL is refused before any
/// request: the URL parser would otherwise strip some of them and send a different path.
#[tokio::test]
async fn get_routing_rejects_a_control_character_in_the_network_before_any_request()
-> anyhow::Result<()> {
    for control in ["\n", "\t", "\r", " ", "\u{0}"] {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;
        let uri = mock.uri();

        for network in [
            format!("/2.3/networks/network-0001{control}"),
            format!("/2.3/networks/net{control}work-0001"),
            format!("{uri}/2.3/networks/network-0001{control}"),
        ] {
            let err = routing_api(&mock)
                .get_routing(&network, None)
                .await
                .expect_err("a control character in the network must be refused");
            assert!(
                matches!(err, Error::Validation { .. }),
                "{network:?} gave {err:?}"
            );
        }
    }
    Ok(())
}

/// A published link carrying a control character is refused rather than silently pinned.
#[tokio::test]
async fn get_routing_rejects_a_control_character_in_a_published_link_instead_of_pinning_it()
-> anyhow::Result<()> {
    for control in ["\n", "\t", "\r", "\u{0}"] {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;

        let link = format!("/2.3/networks/network-0001/routing{control}");
        let parent = json!({"resources": {"routing": link}});
        let err = routing_api(&mock)
            .get_routing("network-0001", Some(&parent))
            .await
            .expect_err("a published link carrying a control character must be refused");
        assert!(
            matches!(err, Error::Validation { ref field, .. } if field == "link"),
            "{control:?} gave {err:?}"
        );
    }
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_routing_with_unknown_network_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist/routing"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = routing_api(&mock);
    let err = api
        .get_routing("does-not-exist", None)
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}
