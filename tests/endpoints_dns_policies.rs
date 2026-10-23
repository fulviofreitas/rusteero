//! HTTP integration tests for `DnsPoliciesApi` (new in v8.0.0), per the crate's testing
//! conventions.

mod common;

use std::sync::Arc;

use rstest::rstest;
use rusteero::endpoints::dns_policies::DnsPoliciesApi;
use rusteero::error::Error;
use serde_json::json;
use tracing::Level;
use wiremock::matchers::{body_json, header, method, path};
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
        .and(user_token_header())
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
        .and(user_token_header())
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
        .and(user_token_header())
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

#[tokio::test]
async fn allow_domain_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/allowed",
        ))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.4/networks/network-0001/dns_policies/network/allowed",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let parent = json!({
        "resources": {
            "dns_policies_network_allowed":
                "/2.4/networks/network-0001/dns_policies/network/allowed"
        }
    });
    api.allow_domain(
        "network-0001",
        "homework.example",
        None,
        None,
        None,
        None,
        Some(&parent),
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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.allow_cnames("network-0001", &["cdn.example"], None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn allow_cnames_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/allowed/cnames",
        ))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.4/networks/network-0001/dns_policies/network/allowed/cnames",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let parent = json!({
        "resources": {
            "dns_policies_network_allowed_cnames":
                "/2.4/networks/network-0001/dns_policies/network/allowed/cnames"
        }
    });
    api.allow_cnames("network-0001", &["cdn.example"], Some(&parent))
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
        .and(user_token_header())
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
        .and(user_token_header())
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

#[tokio::test]
async fn block_domain_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/network/blocked",
        ))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.4/networks/network-0001/dns_policies/network/blocked",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let parent = json!({
        "resources": {
            "dns_policies_network_blocked":
                "/2.4/networks/network-0001/dns_policies/network/blocked"
        }
    });
    api.block_domain("network-0001", "ads.example", None, None, Some(&parent))
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
        .and(user_token_header())
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
        .and(user_token_header())
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

#[tokio::test]
async fn allow_domain_for_profiles_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/allowed",
        ))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.4/networks/network-0001/dns_policies/profiles/allowed",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let parent = json!({
        "resources": {
            "dns_policies_profiles_allowed":
                "/2.4/networks/network-0001/dns_policies/profiles/allowed"
        }
    });
    api.allow_domain_for_profiles(
        "network-0001",
        "homework.example",
        &["profile-0001"],
        None,
        None,
        None,
        None,
        Some(&parent),
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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.allow_cnames_for_profiles("network-0001", &["cdn.example"], &["profile-0001"], None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn allow_cnames_for_profiles_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/allowed/cnames",
        ))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.4/networks/network-0001/dns_policies/profiles/allowed/cnames",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let parent = json!({
        "resources": {
            "dns_policies_profiles_allowed_cnames":
                "/2.4/networks/network-0001/dns_policies/profiles/allowed/cnames"
        }
    });
    api.allow_cnames_for_profiles(
        "network-0001",
        &["cdn.example"],
        &["profile-0001"],
        Some(&parent),
    )
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
        .and(user_token_header())
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
        .and(user_token_header())
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

#[tokio::test]
async fn block_domain_for_profiles_prefers_the_parents_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/dns_policies/profiles/blocked",
        ))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.4/networks/network-0001/dns_policies/profiles/blocked",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let parent = json!({
        "resources": {
            "dns_policies_profiles_blocked":
                "/2.4/networks/network-0001/dns_policies/profiles/blocked"
        }
    });
    api.block_domain_for_profiles(
        "network-0001",
        "ads.example",
        &["profile-0001"],
        None,
        None,
        Some(&parent),
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
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    api.set_profile_blocked_applications("network-0001", "profile-0001", &["app_1", "app_2"])
        .await?;
    Ok(())
}

// ===================== network-level switches (set_network_ad_block / set_network_block_malware) ====

/// The 402 body the live API returns to a network that is not on Eero Plus/Secure.
const PREMIUM_REFUSAL: &str =
    r#"{"meta":{"code":402,"error":"error.premium.user_not_subscribed"}}"#;

/// The "ok" envelope a switch write returns.
fn switch_ok() -> serde_json::Value {
    json!({"meta": {"code": 200}})
}

/// Mounts an authenticated `POST <post_path>` carrying exactly `body` as JSON, expecting `calls`
/// requests.
async fn mount_switch_post(mock: &MockEero, post_path: &str, body: serde_json::Value, calls: u64) {
    Mock::given(method("POST"))
        .and(path(post_path))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(body))
        .respond_with(ResponseTemplate::new(200).set_body_json(switch_ok()))
        .expect(calls)
        .mount(&mock.server)
        .await;
}

/// Whether the mock server has seen no request at all.
async fn received_nothing(mock: &MockEero) -> bool {
    mock.server
        .received_requests()
        .await
        .unwrap_or_default()
        .is_empty()
}

// ----- set_network_ad_block -----

#[rstest]
#[case::on(true)]
#[case::off(false)]
#[tokio::test]
async fn set_network_ad_block_network_wide_body_omits_profiles(
    #[case] enabled: bool,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({ "enable": enabled }),
        1,
    )
    .await;

    let api = dns_policies_api(&mock);
    let env = api
        .set_network_ad_block("network-0001", enabled, None, None)
        .await?;

    assert_eq!(env.into_value(), switch_ok());
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_prefers_the_parents_own_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.3/networks/network-0001/dns_policies/adblock",
        json!({ "enable": true }),
        1,
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/dns_policies/adblock"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001"});
    let api = dns_policies_api(&mock);
    api.set_network_ad_block("network-0001", true, None, Some(&parent))
        .await?;
    Ok(())
}

/// A published url is a base verbatim, not a network id to re-confine: a url that is not
/// `networks/{id}`-shaped still gets the literal `/dns_policies/adblock` suffix.
#[tokio::test]
async fn set_network_ad_block_uses_a_non_network_shaped_parent_url_verbatim() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.4/some/other/shape/dns_policies/adblock",
        json!({ "enable": false }),
        1,
    )
    .await;

    let parent = json!({"data": {"url": "/2.4/some/other/shape"}, "meta": {"code": 200}});
    let api = dns_policies_api(&mock);
    api.set_network_ad_block("network-0001", false, None, Some(&parent))
        .await?;
    Ok(())
}

/// Deliberate divergence from Python: `dns_policies.py:647` joins `f"{base_url}/dns_policies/..."`
/// onto the published url exactly as `self_url` returned it, and `self_url` keeps a trailing
/// slash, so `/2.4/some/other/shape/` yields `/2.4/some/other/shape//dns_policies/adblock` there.
/// This crate drops the trailing slash before joining the suffix, so the request never carries a
/// doubled slash.
#[tokio::test]
async fn set_network_ad_block_drops_a_trailing_slash_from_the_parent_url_before_the_suffix()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.4/some/other/shape/dns_policies/adblock",
        json!({ "enable": false }),
        1,
    )
    .await;

    let parent = json!({"data": {"url": "/2.4/some/other/shape/"}, "meta": {"code": 200}});
    let api = dns_policies_api(&mock);
    api.set_network_ad_block("network-0001", false, None, Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_normalises_bare_ids_to_profile_paths() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({
            "enable": true,
            "profiles": [
                "/2.2/networks/network-0001/profiles/p1",
                "/2.2/networks/network-0001/profiles/p2",
            ],
        }),
        1,
    )
    .await;

    let api = dns_policies_api(&mock);
    api.set_network_ad_block("network-0001", true, Some(&["p1", "p2"]), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_normalises_paths_and_absolute_urls() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({
            "enable": true,
            "profiles": [
                "/2.2/networks/network-0001/profiles/p1",
                "/2.2/networks/network-0001/profiles/p2",
            ],
        }),
        1,
    )
    .await;

    let absolute = format!("{}/2.2/networks/network-0001/profiles/p2", mock.uri());
    let api = dns_policies_api(&mock);
    api.set_network_ad_block(
        "network-0001",
        true,
        Some(&["/2.2/networks/network-0001/profiles/p1", &absolute]),
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_keeps_the_given_profile_order() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({
            "enable": false,
            "profiles": [
                "/2.2/networks/network-0001/profiles/zz",
                "/2.2/networks/network-0001/profiles/aa",
                "/2.2/networks/network-0001/profiles/mm",
            ],
        }),
        1,
    )
    .await;

    let api = dns_policies_api(&mock);
    api.set_network_ad_block("network-0001", false, Some(&["zz", "aa", "mm"]), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_sends_an_empty_profiles_list() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({ "enable": true, "profiles": [] }),
        1,
    )
    .await;

    let api = dns_policies_api(&mock);
    api.set_network_ad_block("network-0001", true, Some(&[]), None)
        .await?;
    Ok(())
}

/// Profile references resolve under `network_id` only: the parent selects the endpoint's base
/// (here API 2.3) but never the profile paths.
#[tokio::test]
async fn set_network_ad_block_resolves_profiles_without_consulting_the_parent() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.3/networks/network-0001/dns_policies/adblock",
        json!({
            "enable": true,
            "profiles": ["/2.2/networks/network-0001/profiles/p1"],
        }),
        1,
    )
    .await;

    let parent = json!({"url": "/2.3/networks/network-0001"});
    let api = dns_policies_api(&mock);
    api.set_network_ad_block("network-0001", true, Some(&["p1"]), Some(&parent))
        .await?;
    Ok(())
}

#[rstest]
#[case::other_network(
    "/2.2/networks/other-network/profiles/p1",
    Some("must belong to the addressed network")
)]
#[case::account_family("/2.2/account", None)]
#[case::device_family("/2.2/networks/network-0001/devices/d1", None)]
#[case::empty("", Some("must be a non-empty string"))]
#[tokio::test]
async fn set_network_ad_block_rejects_foreign_profile_references_before_any_request(
    #[case] profile: &str,
    #[case] message: Option<&str>,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(switch_ok()))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let (_guard, logs) = common::capture_logs();
    let err = api
        .set_network_ad_block("network-0001", true, Some(&["p1", profile]), None)
        .await
        .expect_err("a foreign profile reference must be rejected before any request");

    match &err {
        Error::Validation {
            field,
            message: got,
            ..
        } => {
            assert_eq!(field, "child", "{err:?}");
            if let Some(expected) = message {
                assert_eq!(got, expected);
            }
        }
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(received_nothing(&mock).await);
    // Validation precedes the warning, so a rejected call never claims to issue a write.
    assert_eq!(logs.count(Level::WARN, "not been fully characterised"), 0);
    Ok(())
}

/// Profiles are validated against `network_id`, never against the parent's network, even when
/// the parent's own `url` selects the endpoint.
#[tokio::test]
async fn set_network_ad_block_validates_profiles_against_the_network_id_not_the_parent()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(switch_ok()))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/other-network"});
    let api = dns_policies_api(&mock);
    let err = api
        .set_network_ad_block(
            "network-0001",
            true,
            Some(&["/2.2/networks/other-network/profiles/p1"]),
            Some(&parent),
        )
        .await
        .expect_err("the profile belongs to the parent's network, not to network_id");
    assert!(matches!(err, Error::Validation { field, .. } if field == "child"));
    assert!(received_nothing(&mock).await);
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_logs_the_uncharacterised_write_warning_once() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({ "enable": true, "profiles": ["/2.2/networks/network-0001/profiles/p1"] }),
        1,
    )
    .await;

    let api = dns_policies_api(&mock);
    let (_guard, logs) = common::capture_logs();
    api.set_network_ad_block("network-0001", true, Some(&["p1"]), None)
        .await?;

    assert_eq!(
        logs.count(
            Level::WARN,
            "Issuing write (set network ad block for network): its side effects have not been \
             fully characterised"
        ),
        1,
        "{:?}",
        logs.warnings()
    );
    assert_eq!(logs.warnings().len(), 1);
    // The fixed operation string never carries a caller-supplied identifier.
    assert!(!logs.warnings()[0].contains("network-0001"));
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_premium_required_maps_correctly() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/dns_policies/adblock"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(402).set_body_string(PREMIUM_REFUSAL))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let err = api
        .set_network_ad_block("network-0001", true, None, None)
        .await
        .expect_err("a network without Eero Plus/Secure must surface a premium error");

    assert!(matches!(err, Error::PremiumRequired { .. }), "{err:?}");
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_not_authenticated() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = DnsPoliciesApi::new(Arc::new(mock.transport_anonymous()));
    let err = api
        .set_network_ad_block("network-0001", true, None, None)
        .await
        .expect_err("no session must fail before any request");
    assert!(err.is_auth_error());
    assert!(received_nothing(&mock).await);
    Ok(())
}

// ----- set_network_block_malware -----

#[rstest]
#[case::on(true)]
#[case::off(false)]
#[tokio::test]
async fn set_network_block_malware_posts_the_block_malware_switch(
    #[case] enabled: bool,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/network",
        json!({ "block_malware": enabled }),
        1,
    )
    .await;

    let api = dns_policies_api(&mock);
    let env = api
        .set_network_block_malware("network-0001", enabled, None)
        .await?;

    assert_eq!(env.into_value(), switch_ok());
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_prefers_the_parents_own_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.3/networks/network-0001/dns_policies/network",
        json!({ "block_malware": true }),
        1,
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/dns_policies/network"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let parent = json!({"url": "/2.3/networks/network-0001"});
    let api = dns_policies_api(&mock);
    api.set_network_block_malware("network-0001", true, Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_uses_a_non_network_shaped_parent_url_verbatim()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.4/some/other/shape/dns_policies/network",
        json!({ "block_malware": false }),
        1,
    )
    .await;

    let parent = json!({"url": "/2.4/some/other/shape"});
    let api = dns_policies_api(&mock);
    api.set_network_block_malware("network-0001", false, Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_logs_the_uncharacterised_write_warning_once()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/network",
        json!({ "block_malware": true }),
        1,
    )
    .await;

    let api = dns_policies_api(&mock);
    let (_guard, logs) = common::capture_logs();
    api.set_network_block_malware("network-0001", true, None)
        .await?;

    assert_eq!(
        logs.count(
            Level::WARN,
            "Issuing write (set network DNS policy block_malware for network): its side effects \
             have not been fully characterised"
        ),
        1,
        "{:?}",
        logs.warnings()
    );
    assert_eq!(logs.warnings().len(), 1);
    assert!(!logs.warnings()[0].contains("network-0001"));
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_premium_required_maps_correctly() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/dns_policies/network"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(402).set_body_string(PREMIUM_REFUSAL))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_policies_api(&mock);
    let err = api
        .set_network_block_malware("network-0001", true, None)
        .await
        .expect_err("a network without Eero Plus/Secure must surface a premium error");

    assert!(matches!(err, Error::PremiumRequired { .. }), "{err:?}");
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_not_authenticated() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = DnsPoliciesApi::new(Arc::new(mock.transport_anonymous()));
    let err = api
        .set_network_block_malware("network-0001", true, None)
        .await
        .expect_err("no session must fail before any request");
    assert!(err.is_auth_error());
    assert!(received_nothing(&mock).await);
    Ok(())
}
