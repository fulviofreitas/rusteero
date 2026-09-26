//! HTTP integration tests for `DnsPoliciesApi` (new in v8.0.0), per the crate's testing
//! conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::dns_policies::DnsPoliciesApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

fn dns_policies_api(mock: &MockEero) -> DnsPoliciesApi {
    DnsPoliciesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_advanced_content_filter =====================

#[tokio::test]
async fn get_advanced_content_filter_hits_the_literal_path_by_default() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/advanced_content_filter",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("advanced_content_filter.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let env = api
        .get_advanced_content_filter("network-0001", None)
        .await?;
    assert_eq!(
        env.into_value(),
        fixture_json("advanced_content_filter.json")
    );
    Ok(())
}

#[tokio::test]
async fn get_advanced_content_filter_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.4/networks/network-0001/dns_policies/advanced_content_filter",
        ))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("advanced_content_filter.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let parent = json!({
        "resources": {
            "advanced_content_filter":
                "/2.4/networks/network-0001/dns_policies/advanced_content_filter"
        }
    });
    api.get_advanced_content_filter("network-0001", Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn get_advanced_content_filter_premium_required_maps_correctly() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/advanced_content_filter",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(402).set_body_string(
            r#"{"meta":{"code":402,"error":"error.premium.user_not_subscribed"}}"#,
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let err = api
        .get_advanced_content_filter("network-0001", None)
        .await
        .expect_err("premium-gated network must surface an error");
    assert!(matches!(
        err,
        Error::PremiumRequired { .. } | Error::Api { .. }
    ));
    Ok(())
}

// ===================== allow_domain =====================

#[tokio::test]
async fn allow_domain_sends_only_domain_by_default() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/allowed",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "domain": "homework.example" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.allow_domain(
        "network-0001",
        "homework.example",
        None,
        None,
        None,
        None,
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn allow_domain_sends_only_the_given_optional_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/allowed",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "domain": "homework.example",
            "is_delete": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.allow_domain(
        "network-0001",
        "homework.example",
        None,
        None,
        Some(true),
        None,
        None,
    )
    .await?;
    Ok(())
}

// ===================== allow_cnames =====================

#[tokio::test]
async fn allow_cnames_sends_only_the_domains_field() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/allowed/cnames",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "domains": ["cdn.example"] })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.allow_cnames("network-0001", &["cdn.example"], None)
        .await?;
    Ok(())
}

// ===================== block_domain =====================

#[tokio::test]
async fn block_domain_sends_only_domain_by_default() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/blocked",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "domain": "ads.example" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.block_domain("network-0001", "ads.example", None, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn block_domain_sends_only_the_given_optional_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/blocked",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "domain": "ads.example",
            "keep_profiles": ["profile-0001"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.block_domain(
        "network-0001",
        "ads.example",
        None,
        Some(&["profile-0001"]),
        None,
    )
    .await?;
    Ok(())
}

// ===================== allow_domain_for_profiles =====================

#[tokio::test]
async fn allow_domain_for_profiles_always_sends_domain_and_profiles() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/allowed",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "domain": "homework.example",
            "profiles": ["profile-0001"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.allow_domain_for_profiles(
        "network-0001",
        "homework.example",
        &["profile-0001"],
        None,
        None,
        None,
        None,
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn allow_domain_for_profiles_sends_only_the_given_optional_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/allowed",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "domain": "homework.example",
            "profiles": ["profile-0001"],
            "override": true,
            "reason_to_allow": 3
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.allow_domain_for_profiles(
        "network-0001",
        "homework.example",
        &["profile-0001"],
        Some(true),
        None,
        Some(3),
        None,
        None,
    )
    .await?;
    Ok(())
}

// ===================== allow_cnames_for_profiles =====================

#[tokio::test]
async fn allow_cnames_for_profiles_sends_domains_and_profiles() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/allowed/cnames",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "domains": ["cdn.example"],
            "profiles": ["profile-0001"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.allow_cnames_for_profiles("network-0001", &["cdn.example"], &["profile-0001"], None)
        .await?;
    Ok(())
}

// ===================== block_domain_for_profiles =====================

#[tokio::test]
async fn block_domain_for_profiles_always_sends_domain_and_profiles() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/blocked",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "domain": "ads.example",
            "profiles": ["profile-0001"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.block_domain_for_profiles(
        "network-0001",
        "ads.example",
        &["profile-0001"],
        None,
        None,
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn block_domain_for_profiles_sends_only_the_given_optional_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/blocked",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "domain": "ads.example",
            "profiles": ["profile-0001"],
            "is_delete": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.block_domain_for_profiles(
        "network-0001",
        "ads.example",
        &["profile-0001"],
        Some(true),
        None,
        None,
    )
    .await?;
    Ok(())
}

// ===================== get_profile_applications =====================

#[tokio::test]
async fn get_profile_applications_normalises_a_profile_url_to_its_trailing_id() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/profile-0001/applications",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture("profile_applications.json")),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let env = api
        .get_profile_applications(
            "network-0001",
            "/2.2/networks/network-0001/profiles/profile-0001",
        )
        .await?;
    assert_eq!(env.into_value(), fixture_json("profile_applications.json"));
    Ok(())
}

#[tokio::test]
async fn get_profile_applications_not_authenticated() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = DnsPoliciesApi::new(Arc::new(mock.transport_anonymous()));
    let err = api
        .get_profile_applications("network-0001", "profile-0001")
        .await
        .expect_err("no session must fail before any request");
    assert!(err.is_auth_error());
    Ok(())
}

// ===================== set_profile_blocked_applications =====================

#[tokio::test]
async fn set_profile_blocked_applications_sends_the_full_applications_list() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/profile-0001/applications/blocked",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "applications": ["app_1", "app_2"] })))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.set_profile_blocked_applications("network-0001", "profile-0001", &["app_1", "app_2"])
        .await?;
    Ok(())
}
