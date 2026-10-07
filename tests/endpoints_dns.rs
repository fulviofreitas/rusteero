//! HTTP integration tests for `DnsApi` (`src/endpoints/dns.rs`), v8.0.4, against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! Every authenticated request test pins both `X-User-Token` (primary, v8.0.4) and the legacy
//! `Cookie: s=...` header (secondary, on by default).
//!
//! **Judgment call (test-file split, kept from the pre-v8.0.4 suite)**:
//! `dns_security_and_sqm_settings_all_hit_the_same_network_path` exercises `DnsApi`,
//! `SecurityApi` and `SqmApi` together — `DnsApi::get_dns_settings` resolves via the bare-id
//! `networks/{id}` template unconditionally (it has no `parent` parameter at all), while
//! `SecurityApi`/`SqmApi`'s reads additionally accept (but are not given one here) a `parent` for
//! `self_url` preference; with no `parent` supplied, all three land on the exact same path. This
//! group owns all three domains, so the test lives here (alphabetically first).

mod common;

use std::sync::Arc;

use rusteero::endpoints::dns::DnsApi;
use rusteero::endpoints::security::SecurityApi;
use rusteero::endpoints::sqm::SqmApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

/// Builds a [`DnsApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn dns_api(mock: &MockEero) -> DnsApi {
    DnsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SecurityApi`] pointed at `mock` — used only by the shared-path proof below; see
/// this file's module docs for why.
fn security_api(mock: &MockEero) -> SecurityApi {
    SecurityApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Builds a [`SqmApi`] pointed at `mock` — used only by the shared-path proof below; see this
/// file's module docs for why.
fn sqm_api(mock: &MockEero) -> SqmApi {
    SqmApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A small, obviously-synthetic success envelope every write test in this file can share.
fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

// ===================== get_dns_settings =====================

#[tokio::test]
async fn get_dns_settings_hits_networks_id_and_returns_the_fixture_envelope() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("dns_settings.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.get_dns_settings("network-0001").await?;
    assert_eq!(env.into_value(), fixture_json("dns_settings.json"));
    Ok(())
}

// ===================== shared-path proof =====================

#[tokio::test]
async fn dns_security_and_sqm_settings_all_hit_the_same_network_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(3)
        .mount(&mock.server)
        .await;

    let dns_env = dns_api(&mock).get_dns_settings("network-0001").await?;
    let security_env = security_api(&mock)
        .get_security_settings("network-0001", None)
        .await?;
    let sqm_env = sqm_api(&mock)
        .get_sqm_settings("network-0001", None)
        .await?;

    let expected = fixture_json("network.json");
    assert_eq!(dns_env.into_value(), expected);
    assert_eq!(security_env.into_value(), expected);
    assert_eq!(sqm_env.into_value(), expected);
    Ok(())
}

// ===================== error path =====================

#[tokio::test]
async fn get_dns_settings_with_unknown_id_maps_404_to_api_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/does-not-exist"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such network"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let err = api
        .get_dns_settings("does-not-exist")
        .await
        .expect_err("a 404 must surface as Error::NotFound");

    let Error::NotFound { status, .. } = &err else {
        panic!("expected Error::NotFound, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
    Ok(())
}

// =================================== DnsApi::set_dns_caching ===================================

#[tokio::test]
async fn set_dns_caching_puts_nested_caching_field() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "dns": { "caching": true } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let env = api.set_dns_caching("network-0001", true, None).await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_dns_caching_does_not_send_the_legacy_flat_field() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Exact-match body: `dns_caching` (issue #123's dead field) must never appear.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({ "dns": { "caching": false } })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_dns_caching("network-0001", false, None).await?;
    Ok(())
}

#[tokio::test]
async fn set_dns_caching_prefers_a_parent_supplied_settings_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(body_json(json!({ "dns": { "caching": true } })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = fixture_json("dns_network_with_settings_link.json");
    let api = dns_api(&mock);
    api.set_dns_caching("network-0001", true, Some(&parent))
        .await?;
    Ok(())
}

// =================================== DnsApi::set_custom_dns ===================================

#[tokio::test]
async fn set_custom_dns_ipv4_only_writes_only_the_dns_object() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "custom", "custom": { "ips": ["1.1.1.1", "1.0.0.1"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns("network-0001", &["1.1.1.1", "1.0.0.1"], None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv6_only_writes_only_the_ipv6_object() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "ipv6": { "name_servers": { "mode": "custom", "custom": ["2606:4700:4700::1111"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns("network-0001", &["2606:4700:4700::1111"], None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_dual_stack_configures_both_families_in_one_write() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "custom", "custom": { "ips": ["1.1.1.1", "1.0.0.1"] } },
            "ipv6": {
                "name_servers": {
                    "mode": "custom",
                    "custom": ["2606:4700:4700::1111", "2606:4700:4700::1001"],
                },
            },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns(
        "network-0001",
        &[
            "1.1.1.1",
            "1.0.0.1",
            "2606:4700:4700::1111",
            "2606:4700:4700::1001",
        ],
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_over_limit_raises_and_does_not_write() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns("network-0001", &["1.1.1.1", "1.0.0.1", "8.8.8.8"], None)
        .await
        .expect_err("exceeding the per-family cap must be rejected before any request");
    assert!(matches!(err, Error::Validation { .. }));
    assert!(err.to_string().contains('2'));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_per_family_cap_is_not_a_total_cap() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns(
        "network-0001",
        &[
            "1.1.1.1",
            "1.0.0.1",
            "2606:4700:4700::1111",
            "2606:4700:4700::1001",
        ],
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_empty_list_raises_without_writing() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns("network-0001", &[], None)
        .await
        .expect_err("an empty list must be rejected rather than silently clearing DNS");
    match &err {
        Error::Validation { field, message, .. } => {
            assert_eq!(field, "dns_servers");
            assert!(message.contains("clear_custom_dns"));
        }
        other => panic!("expected Error::Validation, got {other:?}"),
    }

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

// ============================== DnsApi::set_custom_dns_ipv4 / ipv6 ==============================

#[tokio::test]
async fn set_custom_dns_ipv4_leaves_ipv6_untouched() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "custom", "custom": { "ips": ["8.8.8.8", "8.8.4.4"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns_ipv4("network-0001", &["8.8.8.8", "8.8.4.4"], None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv6_leaves_ipv4_untouched() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "ipv6": { "name_servers": { "mode": "custom", "custom": ["2606:4700:4700::1111"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns_ipv6("network-0001", &["2606:4700:4700::1111"], None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv4_rejects_an_ipv6_literal() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns_ipv4("network-0001", &["2606:4700:4700::1111"], None)
        .await
        .expect_err("a family mismatch must be reported distinctly from malformed input");
    let Error::Validation { message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert!(message.contains("IPv6"));
    assert!(message.contains("expected IPv4"));
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv6_rejects_an_ipv4_literal() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns_ipv6("network-0001", &["1.1.1.1"], None)
        .await
        .expect_err("a family mismatch must be reported distinctly from malformed input");
    let Error::Validation { message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert!(message.contains("expected IPv6"));
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv6_compressed_form_is_sent_on_the_wire() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // The API stores addresses fully expanded, but this crate always sends the compressed
    // (RFC 5952) form on the wire — matching Python's `str(ipaddress.ip_address(...))`.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "ipv6": { "name_servers": { "mode": "custom", "custom": ["2606:4700:4700::1111"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns_ipv6("network-0001", &["2606:4700:4700:0:0:0:0:1111"], None)
        .await?;
    Ok(())
}

/// An IPv4-mapped IPv6 literal must be serialised in Python's hex-group
/// form (`ipaddress.IPv6Address("::ffff:192.168.1.1")` -> `"::ffff:c0a8:101"`), not Rust
/// `Ipv6Addr::Display`'s dotted-quad special case (`"::ffff:192.168.1.1"`).
#[tokio::test]
async fn set_custom_dns_ipv6_serialises_an_ipv4_mapped_literal_in_hex_form() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "ipv6": { "name_servers": { "mode": "custom", "custom": ["::ffff:c0a8:101"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns_ipv6("network-0001", &["::ffff:192.168.1.1"], None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv4_empty_list_points_at_clear_custom_dns() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns_ipv4("network-0001", &[], None)
        .await
        .expect_err("an empty per-family list must be rejected");
    let Error::Validation { message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert!(message.contains("clear_custom_dns"));
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_ipv6_empty_list_points_at_clear_custom_dns() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns_ipv6("network-0001", &[], None)
        .await
        .expect_err("an empty per-family list must be rejected");
    let Error::Validation { field, message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "dns_servers");
    assert!(message.contains("clear_custom_dns(family='ipv6')"));
    Ok(())
}

/// Ported from `_validate_servers` (`dns.py:105-108`): a literal that does not parse as any IP
/// address at all is rejected with `'{entry}' is not a valid IP address`.
#[tokio::test]
async fn set_custom_dns_ipv4_rejects_an_unparseable_literal() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns_ipv4("network-0001", &["1.2.3.x"], None)
        .await
        .expect_err("a non-IP literal must be rejected");
    let Error::Validation { field, message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "dns_servers");
    assert_eq!(message, "'1.2.3.x' is not a valid IP address");
    Ok(())
}

/// Ported from `_validate_servers` (`dns.py:112-116`): a `%`-scoped literal is meaningless to a
/// cloud API and rejected locally, before any request is sent.
#[tokio::test]
async fn set_custom_dns_ipv6_rejects_a_zone_scoped_literal() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns_ipv6("network-0001", &["fe80::1%eth0"], None)
        .await
        .expect_err("a zone-scoped literal must be rejected");
    let Error::Validation { field, message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "dns_servers");
    assert_eq!(
        message,
        "'fe80::1%eth0' has a zone identifier, which is not valid for a DNS server"
    );
    Ok(())
}

/// Ported from `_validate_servers`'s dedicated empty-string check (`dns.py:105-108`), reached
/// directly by `set_custom_dns_ipv4`/`set_custom_dns_ipv6` (unlike `set_custom_dns`, which
/// partitions by family via `_split_by_family` first — see the next test for that divergence).
#[tokio::test]
async fn set_custom_dns_ipv4_rejects_an_empty_entry_with_the_dedicated_message()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns_ipv4("network-0001", &["1.1.1.1", "  "], None)
        .await
        .expect_err("a blank entry must be rejected");
    let Error::Validation { field, message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "dns_servers");
    assert_eq!(message, "IP address must not be empty");
    Ok(())
}

/// `set_custom_dns` (unlike `set_custom_dns_ipv4`/`ipv6`) partitions its mixed list via
/// `_split_by_family` before validating each family, and `_split_by_family` has no dedicated
/// empty-string check (`dns.py:139-165`) — an empty entry falls through to the generic
/// parse-failure message instead of `_validate_servers`'s `"must not be empty"` wording.
#[tokio::test]
async fn set_custom_dns_rejects_an_empty_entry_with_the_split_message() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_custom_dns("network-0001", &["1.1.1.1", ""], None)
        .await
        .expect_err("a blank entry must be rejected");
    let Error::Validation { field, message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "dns_servers");
    assert_eq!(message, "'' is not a valid IP address");
    Ok(())
}

// ================================== DnsApi::clear_custom_dns ==================================

#[tokio::test]
async fn clear_custom_dns_clears_both_families_by_default() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "automatic" },
            "ipv6": { "name_servers": { "mode": "automatic" } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.clear_custom_dns("network-0001", None, None).await?;
    Ok(())
}

#[tokio::test]
async fn clear_custom_dns_is_non_destructive_no_custom_key_sent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Exact-match body with no `custom` key anywhere: sending one (even empty) would erase the
    // servers the API retains.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "automatic" },
            "ipv6": { "name_servers": { "mode": "automatic" } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.clear_custom_dns("network-0001", None, None).await?;
    Ok(())
}

#[tokio::test]
async fn clear_custom_dns_ipv4_only_leaves_ipv6_selector_alone() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({ "dns": { "mode": "automatic" } })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.clear_custom_dns("network-0001", Some("ipv4"), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn clear_custom_dns_ipv6_only_leaves_ipv4_selector_alone() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(
            json!({ "ipv6": { "name_servers": { "mode": "automatic" } } }),
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.clear_custom_dns("network-0001", Some("ipv6"), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn clear_custom_dns_invalid_family_raises_without_writing() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .clear_custom_dns("network-0001", Some("ipv5"), None)
        .await
        .expect_err("an unknown family must be rejected");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "family"));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

// ===================================== DnsApi::set_dns_mode =====================================

#[tokio::test]
async fn set_dns_mode_provider_names_are_rejected_as_modes() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    for preset in ["cloudflare", "google", "opendns", "quad9"] {
        let err = api
            .set_dns_mode("network-0001", preset, None, None)
            .await
            .expect_err(&format!("{preset:?} must not be accepted as a mode"));
        let Error::Validation { message, .. } = &err else {
            panic!("expected Error::Validation, got {err:?}");
        };
        assert!(message.contains("default_test_servers"));
    }

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_auto_switches_mode_without_discarding_servers() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "automatic" },
            "ipv6": { "name_servers": { "mode": "automatic" } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(3)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    for mode in ["auto", "automatic", "AUTO"] {
        api.set_dns_mode("network-0001", mode, None, None).await?;
    }
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_custom_with_servers_forwards_them() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "custom", "custom": { "ips": ["9.9.9.9"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    let servers = ["9.9.9.9"];
    api.set_dns_mode("network-0001", "custom", Some(&servers), None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_custom_without_servers_reenables_stored_servers() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Exact-match body: mode-only, no `custom` key on either family — sending one would erase
    // the servers this write is meant to re-enable.
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "custom" },
            "ipv6": { "name_servers": { "mode": "custom" } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_dns_mode("network-0001", "custom", None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_dns_mode_invalid_mode_is_validation_error_with_no_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dns_api(&mock);
    let err = api
        .set_dns_mode("network-0001", "not-a-real-mode", None, None)
        .await
        .expect_err("an unrecognised mode must be rejected before any request");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "mode"));

    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
    Ok(())
}

// ===================== parent link preference (Resource/link-wiring proofs) =====================

#[tokio::test]
async fn set_custom_dns_prefers_a_parent_supplied_settings_link_over_the_template()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.4/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "custom", "custom": { "ips": ["1.1.1.1"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = fixture_json("dns_network_with_settings_link.json");
    let api = dns_api(&mock);
    api.set_custom_dns("network-0001", &["1.1.1.1"], Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_custom_dns_falls_back_to_the_template_with_a_bare_id_and_no_parent()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(body_json(json!({
            "dns": { "mode": "custom", "custom": { "ips": ["1.1.1.1"] } },
        })))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dns_api(&mock);
    api.set_custom_dns("network-0001", &["1.1.1.1"], None)
        .await?;
    Ok(())
}
