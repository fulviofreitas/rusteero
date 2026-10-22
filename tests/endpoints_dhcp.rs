//! HTTP integration tests for `DhcpApi` (`src/endpoints/dhcp.rs`) against a local `wiremock`
//! server per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::dhcp::{
    DHCP_MODE_AUTOMATIC, DHCP_MODE_CUSTOM, DHCP_MODE_MANUAL, DHCP_MODES, DhcpApi,
};
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

fn dhcp_api(mock: &MockEero) -> DhcpApi {
    DhcpApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== set_dhcp =====================

#[tokio::test]
async fn set_dhcp_mode_only_sends_just_the_mode_field() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "dhcp": { "mode": "automatic" } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_dhcp("network-0001", Some("automatic"), None, None, None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_dhcp_custom_only_given_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    let custom = json!({ "start_ip": "192.0.2.10" });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(
            json!({ "dhcp": { "custom": { "start_ip": "192.0.2.10" } } }),
        ))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_dhcp(
            "network-0001",
            None,
            Some(custom.as_object().unwrap()),
            None,
            None,
        )
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_dhcp_custom_v2_only_given_keys() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    let custom_v2 = json!({ "supernet": "10.0.0.0/8" });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(
            json!({ "dhcp": { "custom_v2": { "supernet": "10.0.0.0/8" } } }),
        ))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_dhcp(
            "network-0001",
            None,
            None,
            Some(custom_v2.as_object().unwrap()),
            None,
        )
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_dhcp_rejects_invalid_mode() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Validation is case-sensitive and happens before any request.
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    let api = dhcp_api(&mock);

    let err = api
        .set_dhcp("network-0001", Some("AUTOMATIC"), None, None, None)
        .await
        .unwrap_err();

    match err {
        Error::Validation { field, message, .. } => {
            assert_eq!(field, "mode");
            assert_eq!(
                message,
                "must be one of ['automatic', 'custom', 'manual'], got 'AUTOMATIC'"
            );
        }
        other => panic!("expected a validation error, got {other:?}"),
    }
    Ok(())
}

#[test]
fn the_mode_constants_name_the_three_accepted_modes() {
    assert_eq!(DHCP_MODE_AUTOMATIC, "automatic");
    assert_eq!(DHCP_MODE_MANUAL, "manual");
    assert_eq!(DHCP_MODE_CUSTOM, "custom");
    assert_eq!(DHCP_MODES, ["automatic", "manual", "custom"]);
}

#[tokio::test]
async fn set_dhcp_custom_mode_sends_custom() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "dhcp": { "mode": "custom" } })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    dhcp_api(&mock)
        .set_dhcp("network-0001", Some("custom"), None, None, None)
        .await?;
    Ok(())
}

/// The API refuses a literal `"manual"` (`dhcp.mode: error.form.enum.invalid`); the caller-facing
/// mode is kept for compatibility and goes out as `"custom"`.
#[tokio::test]
async fn set_dhcp_manual_mode_is_sent_as_custom_on_the_wire() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({ "dhcp": { "mode": "custom" } })))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    dhcp_api(&mock)
        .set_dhcp("network-0001", Some("manual"), None, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_dhcp_manual_mode_with_a_lease_range_sends_custom_and_the_range() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let custom = json!({
        "start_ip": "192.168.4.10",
        "end_ip": "192.168.4.99",
        "subnet_ip": "192.168.4.0",
        "subnet_mask": "255.255.255.0",
    });
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(
            json!({ "dhcp": { "mode": "custom", "custom": custom } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    dhcp_api(&mock)
        .set_dhcp(
            "network-0001",
            Some("manual"),
            custom.as_object(),
            None,
            None,
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_dhcp_rejects_unknown_custom_field() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dhcp_api(&mock);
    let custom = json!({ "bogus_field": "x" });

    let err = api
        .set_dhcp(
            "network-0001",
            None,
            Some(custom.as_object().unwrap()),
            None,
            None,
        )
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { field, .. } if field == "field"));
    Ok(())
}

#[tokio::test]
async fn set_dhcp_rejects_unknown_custom_v2_field() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dhcp_api(&mock);
    let custom_v2 = json!({ "bogus_field": "x" });

    let err = api
        .set_dhcp(
            "network-0001",
            None,
            None,
            Some(custom_v2.as_object().unwrap()),
            None,
        )
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { field, .. } if field == "field"));
    Ok(())
}

#[tokio::test]
async fn set_dhcp_rejects_no_fields_supplied() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dhcp_api(&mock);

    let err = api
        .set_dhcp("network-0001", None, None, None, None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { field, .. } if field == "dhcp"));
    Ok(())
}

#[tokio::test]
async fn set_dhcp_prefers_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {"settings": "/2.3/networks/network-0001/settings"}});
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_dhcp("network-0001", Some("manual"), None, None, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_connection_mode =====================

#[tokio::test]
async fn set_connection_mode_bridge_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "connection": { "mode": "BRIDGE" } })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_connection_mode("network-0001", "BRIDGE", None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_connection_mode_nat_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "connection": { "mode": "NAT" } })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api.set_connection_mode("network-0001", "NAT", None).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_connection_mode_rejects_invalid_mode() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let api = dhcp_api(&mock);

    let err = api
        .set_connection_mode("network-0001", "ROUTER", None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Validation { field, .. } if field == "mode"));
    Ok(())
}

#[tokio::test]
async fn set_connection_mode_prefers_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {"settings": "/2.3/networks/network-0001/settings"}});
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_connection_mode("network-0001", "BRIDGE", Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_nat_port_randomization =====================

#[tokio::test]
async fn set_nat_port_randomization_true_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "nat_port_randomization": true })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_nat_port_randomization("network-0001", true, None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_nat_port_randomization_false_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .and(body_json(json!({ "nat_port_randomization": false })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_nat_port_randomization("network-0001", false, None)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_nat_port_randomization_prefers_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent = json!({"resources": {"settings": "/2.3/networks/network-0001/settings"}});
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/settings"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_nat_port_randomization("network-0001", true, Some(&parent))
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_pppoe =====================

#[tokio::test]
async fn set_pppoe_sends_nested_json_to_eero_pppoe_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": { "pppoe_credentials": "encrypted-blob" },
    });

    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-serial-0001/pppoe"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({
            "pppoe": { "username": "isp-user", "password": "isp-secret" }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = dhcp_api(&mock);
    let env = api
        .set_pppoe("eero-serial-0001", "isp-user", "isp-secret")
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}
