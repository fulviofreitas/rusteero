//! `Client` integration suite for `DnsPoliciesApi` (new in v8.0.0) — cache invalidation and
//! `network_id` resolution, per the crate's testing conventions.

mod common;

use rstest::rstest;
use tracing::Level;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;
use serde_json::{Value, json};

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

// ===================== network-level switches (set_network_ad_block / set_network_block_malware) ====

/// The 402/403 body the live API returns to a network that is not on Eero Plus/Secure.
const PREMIUM_REFUSAL: &str =
    r#"{"meta":{"code":402,"error":"error.premium.user_not_subscribed"}}"#;

/// The "ok" envelope a switch write returns.
fn switch_ok() -> Value {
    json!({"meta": {"code": 200}})
}

/// Mounts `GET <get_path>` answering `body`, authenticated, expecting `calls` requests.
async fn mount_get(mock: &MockEero, get_path: &str, body: String, calls: u64) {
    Mock::given(method("GET"))
        .and(path(get_path))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(calls)
        .mount(&mock.server)
        .await;
}

/// Mounts the authenticated `POST <post_path>` carrying exactly `body` as JSON, expecting
/// `calls` requests.
async fn mount_switch_post(mock: &MockEero, post_path: &str, body: Value, calls: u64) {
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

/// Mounts a trap on `POST <post_path>` that must never be reached.
async fn mount_post_trap(mock: &MockEero, post_path: &str) {
    Mock::given(method("POST"))
        .and(path(post_path))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
}

/// Mounts the cached-read endpoints the cache tests prime: the network, two profiles and the
/// profile list of `network-0001`, and one profile of `network-0002`, each expecting the given
/// number of requests: a count of 2 means the entry is dropped and refetched, 1 that it stays
/// cached.
async fn mount_cached_reads(
    mock: &MockEero,
    network_calls: u64,
    own_profile_calls: u64,
    other_network_profile_calls: u64,
) {
    mount_get(
        mock,
        "/2.2/networks/network-0001",
        fixture("network.json"),
        network_calls,
    )
    .await;
    mount_get(
        mock,
        "/2.2/networks/network-0001/profiles/profile-0001",
        fixture("profile.json"),
        own_profile_calls,
    )
    .await;
    mount_get(
        mock,
        "/2.2/networks/network-0001/profiles/profile-0002",
        fixture("profile.json"),
        own_profile_calls,
    )
    .await;
    mount_get(
        mock,
        "/2.2/networks/network-0001/profiles",
        fixture("profiles.json"),
        own_profile_calls,
    )
    .await;
    mount_get(
        mock,
        "/2.2/networks/network-0002/profiles/profile-0001",
        fixture("profile.json"),
        other_network_profile_calls,
    )
    .await;
}

/// Reads everything [`mount_cached_reads`] serves, through the cache.
async fn read_cached(client: &Client) -> Result<(), Error> {
    client.get_network(Some("network-0001"), false).await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .get_profile("profile-0002", Some("network-0001"), false)
        .await?;
    client.get_profiles(Some("network-0001"), false).await?;
    client
        .get_profile("profile-0001", Some("network-0002"), false)
        .await?;
    Ok(())
}

// ----- set_network_ad_block -----

#[tokio::test]
async fn set_network_ad_block_network_wide_keeps_profile_caches() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({ "enable": true }),
        1,
    )
    .await;
    // Only the network is refetched; every profile entry survives.
    mount_cached_reads(&mock, 2, 1, 1).await;

    let client = client(&mock).await;
    read_cached(&client).await?;
    let env = client
        .set_network_ad_block(true, None, Some("network-0001"), None)
        .await?;
    assert_eq!(env.into_value(), switch_ok());
    read_cached(&client).await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_with_profiles_evicts_every_profile_of_the_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({
            "enable": true,
            "profiles": ["/2.2/networks/network-0001/profiles/profile-0001"],
        }),
        1,
    )
    .await;
    // Both profiles and the list of the addressed network are refetched, although the call named
    // only one profile; the other network's profile is not.
    mount_cached_reads(&mock, 2, 2, 1).await;

    let client = client(&mock).await;
    read_cached(&client).await?;
    client
        .set_network_ad_block(true, Some(&["profile-0001"]), Some("network-0001"), None)
        .await?;
    read_cached(&client).await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_with_an_empty_profiles_list_sends_it_and_still_evicts()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({ "enable": true, "profiles": [] }),
        1,
    )
    .await;
    mount_cached_reads(&mock, 2, 2, 1).await;

    let client = client(&mock).await;
    read_cached(&client).await?;
    client
        .set_network_ad_block(true, Some(&[]), Some("network-0001"), None)
        .await?;
    read_cached(&client).await?;
    Ok(())
}

#[rstest]
#[case::payment_required(402)]
#[case::forbidden(403)]
#[tokio::test]
async fn set_network_ad_block_premium_error_keeps_the_cache(
    #[case] status: u16,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/dns_policies/adblock"))
        .respond_with(ResponseTemplate::new(status).set_body_string(PREMIUM_REFUSAL))
        .expect(1)
        .mount(&mock.server)
        .await;
    // Nothing is refetched: the network and every profile stay cached.
    mount_cached_reads(&mock, 1, 1, 1).await;

    let client = client(&mock).await;
    read_cached(&client).await?;
    let err = client
        .set_network_ad_block(true, Some(&["profile-0001"]), Some("network-0001"), None)
        .await
        .expect_err("a network without Eero Plus/Secure must surface a premium error");
    assert!(matches!(err, Error::PremiumRequired { .. }), "{err:?}");
    read_cached(&client).await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_rejects_a_foreign_profile_before_any_write() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_post_trap(&mock, "/2.2/networks/network-0001/dns_policies/adblock").await;
    mount_cached_reads(&mock, 1, 1, 1).await;

    let client = client(&mock).await;
    read_cached(&client).await?;
    let err = client
        .set_network_ad_block(
            true,
            Some(&["/2.2/networks/other-network/profiles/profile-0001"]),
            Some("network-0001"),
            None,
        )
        .await
        .expect_err("a profile of another network must be rejected");
    assert!(
        matches!(&err, Error::Validation { field, .. } if field == "child"),
        "{err:?}"
    );
    read_cached(&client).await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_logs_the_uncharacterised_write_warning_once() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/adblock",
        json!({ "enable": false }),
        1,
    )
    .await;

    let client = client(&mock).await;
    let (_guard, logs) = common::capture_logs();
    client
        .set_network_ad_block(false, None, Some("network-0001"), None)
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
    Ok(())
}

/// The caller's `parent` wins over the cached network envelope, and its own `url` is the base.
#[tokio::test]
async fn set_network_ad_block_prefers_the_callers_parent_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        json!({
            "meta": {"code": 200},
            "data": {"id": "network-0001", "url": "/2.4/networks/network-0001"},
        })
        .to_string(),
        1,
    )
    .await;
    mount_switch_post(
        &mock,
        "/2.3/networks/network-0001/dns_policies/adblock",
        json!({ "enable": true }),
        1,
    )
    .await;
    mount_post_trap(&mock, "/2.4/networks/network-0001/dns_policies/adblock").await;
    mount_post_trap(&mock, "/2.2/networks/network-0001/dns_policies/adblock").await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let parent = json!({"url": "/2.3/networks/network-0001"});
    client
        .set_network_ad_block(true, None, Some("network-0001"), Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_uses_the_cached_networks_url_without_a_caller_parent()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        json!({
            "meta": {"code": 200},
            "data": {"id": "network-0001", "url": "/2.4/networks/network-0001"},
        })
        .to_string(),
        1,
    )
    .await;
    mount_switch_post(
        &mock,
        "/2.4/networks/network-0001/dns_policies/adblock",
        json!({ "enable": true }),
        1,
    )
    .await;
    mount_post_trap(&mock, "/2.2/networks/network-0001/dns_policies/adblock").await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_network_ad_block(true, None, Some("network-0001"), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_ad_block_resolves_the_network_without_auto_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(&mock, "/2.2/networks", "{}".to_owned(), 0).await;

    let client = client(&mock).await;
    let err = client
        .set_network_ad_block(true, None, None, None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    Ok(())
}

// ----- set_network_block_malware -----

#[tokio::test]
async fn set_network_block_malware_posts_the_block_malware_key() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/network",
        json!({ "block_malware": false }),
        1,
    )
    .await;

    let client = client(&mock).await;
    let env = client
        .set_network_block_malware(false, Some("network-0001"), None)
        .await?;
    assert_eq!(env.into_value(), switch_ok());
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_evicts_the_network_but_not_profiles() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_switch_post(
        &mock,
        "/2.2/networks/network-0001/dns_policies/network",
        json!({ "block_malware": true }),
        1,
    )
    .await;
    mount_cached_reads(&mock, 2, 1, 1).await;

    let client = client(&mock).await;
    read_cached(&client).await?;
    client
        .set_network_block_malware(true, Some("network-0001"), None)
        .await?;
    read_cached(&client).await?;
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_premium_error_keeps_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/dns_policies/network"))
        .respond_with(ResponseTemplate::new(402).set_body_string(PREMIUM_REFUSAL))
        .expect(1)
        .mount(&mock.server)
        .await;
    mount_cached_reads(&mock, 1, 1, 1).await;

    let client = client(&mock).await;
    read_cached(&client).await?;
    let err = client
        .set_network_block_malware(true, Some("network-0001"), None)
        .await
        .expect_err("a network without Eero Plus/Secure must surface a premium error");
    assert!(matches!(err, Error::PremiumRequired { .. }), "{err:?}");
    read_cached(&client).await?;
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_prefers_the_callers_parent_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        json!({
            "meta": {"code": 200},
            "data": {"id": "network-0001", "url": "/2.4/networks/network-0001"},
        })
        .to_string(),
        1,
    )
    .await;
    mount_switch_post(
        &mock,
        "/2.3/networks/network-0001/dns_policies/network",
        json!({ "block_malware": true }),
        1,
    )
    .await;
    mount_post_trap(&mock, "/2.4/networks/network-0001/dns_policies/network").await;
    mount_post_trap(&mock, "/2.2/networks/network-0001/dns_policies/network").await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let parent = json!({"url": "/2.3/networks/network-0001"});
    client
        .set_network_block_malware(true, Some("network-0001"), Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_network_block_malware_resolves_the_network_without_auto_discovery()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(&mock, "/2.2/networks", "{}".to_owned(), 0).await;

    let client = client(&mock).await;
    let err = client
        .set_network_block_malware(true, None, None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    Ok(())
}
