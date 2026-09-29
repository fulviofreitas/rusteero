//! `Client` integration suite for `DnsPoliciesApi` (new in v8.0.0) — cache invalidation and
//! `network_id` resolution, per the crate's testing conventions.

mod common;

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use serde_json::json;

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

#[tokio::test]
async fn get_advanced_content_filter_reaches_the_literal_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/advanced_content_filter",
        ))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("advanced_content_filter.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_advanced_content_filter(Some("network-0001"))
        .await?;
    Ok(())
}

/// `allow_domain_for_profiles` invalidates `profiles[{nid}_profiles]`, not `network[{nid}]` —
/// the invalidation-target/parent-source mismatch `client.md` §4 flags explicitly.
#[tokio::test]
async fn allow_domain_for_profiles_invalidates_the_profiles_list_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/allowed",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_profiles(Some("network-0001"), false).await?;
    client
        .allow_domain_for_profiles(
            "homework.example",
            &["profile-0001"],
            None,
            None,
            None,
            None,
            Some("network-0001"),
        )
        .await?;
    client.get_profiles(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn allow_domain_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/allowed",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "domain": "homework.example" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .allow_domain(
            "homework.example",
            None,
            None,
            None,
            None,
            Some("network-0001"),
        )
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn set_profile_blocked_applications_invalidates_the_profile_and_list_buckets()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/profile-0001/applications/blocked",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .set_profile_blocked_applications("profile-0001", &["app_1"], Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}
