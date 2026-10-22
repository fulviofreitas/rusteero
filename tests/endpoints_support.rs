//! HTTP integration tests for `SupportApi` (`src/endpoints/support.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! `request_support` carries a nested object to prove a passthrough body arrives byte-identical,
//! nesting included.

mod common;

use std::sync::Arc;

use rusteero::endpoints::support::SupportApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{any, body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn support_api(mock: &MockEero) -> SupportApi {
    SupportApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Every spelling of the network `get_support` must pin to 2.2: a bare id, API paths and absolute
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

// ===================== get_support =====================

#[tokio::test]
async fn get_support_bare_id_falls_back_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": { "code": 200 },
        "data": { "phone": "+15555550100", "email": "support@example.com" },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = support_api(&mock);
    let env = api.get_support("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_support_prefers_the_parents_support_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/support"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/custom-support"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"support": "/2.2/networks/network-0001/custom-support"}});
    let api = support_api(&mock);
    let env = api.get_support("network-0001", Some(&parent)).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_support_pins_to_2_2_even_with_2_3_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "email": "support@example.com" } });
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/support"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"support": "/2.3/networks/network-0001/support"}});
    let env = support_api(&mock)
        .get_support("network-0001", Some(&parent))
        .await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_support_pins_a_published_link_on_any_version_to_2_2() -> anyhow::Result<()> {
    for version in ["2.2", "2.3", "2.4", "10.25"] {
        let mock = MockEero::start().await;
        Mock::given(method("GET"))
            .and(path("/2.2/networks/network-0001/support"))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&mock.server)
            .await;

        let link = format!("/{version}/networks/network-0001/support");
        let parent = json!({"resources": {"support": link}});
        support_api(&mock)
            .get_support("network-0001", Some(&parent))
            .await
            .unwrap_or_else(|err| panic!("published link on {version}: {err}"));
    }
    Ok(())
}

#[tokio::test]
async fn get_support_pins_to_2_2_for_every_network_form() -> anyhow::Result<()> {
    for form in NETWORK_FORMS {
        let mock = MockEero::start().await;
        Mock::given(method("GET"))
            .and(path("/2.3/networks/network-0001/support"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock.server)
            .await;
        Mock::given(method("GET"))
            .and(path("/2.2/networks/network-0001/support"))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&mock.server)
            .await;

        let network = network_form(&mock.uri(), form);
        support_api(&mock)
            .get_support(&network, None)
            .await
            .unwrap_or_else(|err| panic!("network form {form}: {err}"));
    }
    Ok(())
}

/// A whitespace or control character in a caller's network path or URL is refused before any
/// request: the URL parser would otherwise strip some of them and send a different path.
///
/// Passes only once `links::resource_url` itself refuses such characters in a caller's path or URL.
#[tokio::test]
async fn get_support_rejects_a_control_character_in_the_network_before_any_request()
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
            let err = support_api(&mock)
                .get_support(&network, None)
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
///
/// Passes only once the plain-path rule for published links refuses control characters.
#[tokio::test]
async fn get_support_rejects_a_control_character_in_a_published_link_instead_of_pinning_it()
-> anyhow::Result<()> {
    for control in ["\n", "\t", "\r", "\u{0}"] {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;

        let link = format!("/2.3/networks/network-0001/support{control}");
        let parent = json!({"resources": {"support": link}});
        let err = support_api(&mock)
            .get_support("network-0001", Some(&parent))
            .await
            .expect_err("a published link carrying a control character must be refused");
        assert!(
            matches!(err, Error::Validation { ref field, .. } if field == "link"),
            "{control:?} gave {err:?}"
        );
    }
    Ok(())
}

// ===================== request_support =====================

#[tokio::test]
async fn request_support_passthrough_body_arrives_byte_identical_including_nested_object()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "issue": "wifi-dropping",
        "contact": { "email": "user@example.com", "phone": "+15555550100" },
    });
    let response = json!({ "meta": { "code": 200 }, "data": { "ticket_id": "tick-0001" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/support"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = support_api(&mock);
    let env = api.request_support("network-0001", payload, None).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn request_support_prefers_the_parents_support_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({"issue": "wifi-dropping"});
    let response = json!({ "meta": { "code": 200 }, "data": { "ticket_id": "tick-0002" } });

    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/support"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/custom-support"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"support": "/2.2/networks/network-0001/custom-support"}});
    let api = support_api(&mock);
    let env = api
        .request_support("network-0001", payload, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn request_support_is_not_version_pinned() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({"issue": "wifi-dropping"});
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/support"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.3/networks/network-0001/support"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"support": "/2.3/networks/network-0001/support"}});
    support_api(&mock)
        .request_support("network-0001", payload, Some(&parent))
        .await?;
    Ok(())
}
