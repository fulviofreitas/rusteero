//! HTTP integration tests for `WanApi` (`src/endpoints/wan.rs`) against a local `wiremock`
//! server per the crate's testing conventions. Every endpoint here is served on API version 2.3.

mod common;

use std::sync::Arc;

use rusteero::endpoints::wan::WanApi;
use rusteero::error::Error;
use serde_json::{Value, json};
use tracing::Level;
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, capture_logs, session_cookie, user_token_header};

fn wan_api(mock: &MockEero) -> WanApi {
    WanApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// Every spelling of the network the WAN calls must pin to 2.3: a bare id, API paths and absolute
/// API-host URLs on either version, and absolute URLs whose scheme is not lower case.
const NETWORK_FORMS: [&str; 7] = [
    "id",
    "path-2.2",
    "path-2.3",
    "url-2.2",
    "url-2.3",
    "upper-case-scheme-url-2.2",
    "mixed-case-scheme-url-2.2",
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
        "upper-case-scheme-url-2.2" => format!("{upper}/2.2/networks/network-0001"),
        "mixed-case-scheme-url-2.2" => format!("{mixed}/2.2/networks/network-0001"),
        other => panic!("unknown network form {other}"),
    }
}

/// Mounts the pair every version-pin test needs: `tail` on API 2.2 must see no request at all, and
/// on API 2.3 it must see exactly one authenticated request (carrying `body` when given).
async fn mount_pinned(mock: &MockEero, verb: &str, tail: &str, body: Option<Value>) {
    Mock::given(method(verb))
        .and(path(format!("/2.2/{tail}")))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    let pinned = Mock::given(method(verb))
        .and(path(format!("/2.3/{tail}")))
        .and(session_cookie())
        .and(user_token_header());
    let pinned = match body {
        Some(body) => pinned.and(body_json(body)),
        None => pinned,
    };
    pinned
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;
}

/// Mounts a catch-all mock that fails the test if it receives any request at all.
async fn expect_no_request(mock: &MockEero) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
}

// ===================== get_multistaticip =====================

#[tokio::test]
async fn get_multistaticip_uses_version_2_3() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "enabled": true, "type": "P" } });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api.get_multistaticip("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_multistaticip_prefers_parent_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let parent =
        json!({"resources": {"multistaticip": "/2.3/networks/network-0001/multistaticip"}});
    let body = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api.get_multistaticip("network-0001", Some(&parent)).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_multistaticip_raises_not_found_on_404() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {
            "code": 404,
            "error": "error.network.multistaticip_not_found",
            "message": "error.network.multistaticip_not_found",
        },
        "data": null,
    });

    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(404).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let err = api
        .get_multistaticip("network-0001", None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::NotFound { status: 404, .. }));
    Ok(())
}

// ===================== set_multistaticip =====================

#[tokio::test]
async fn set_multistaticip_forwards_body_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "enabled": true,
        "type": "P",
        "multistaticip_settings": { "router_ip": "203.0.113.1" },
    });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/multistaticip"))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api.set_multistaticip("network-0001", payload).await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_secondary_wan_config =====================

#[tokio::test]
async fn set_secondary_wan_config_forwards_body_unchanged() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let payload = json!({
        "devices": [{ "mac": "aa:bb:cc:00:00:01", "secondary_wan_deny_access": true }],
    });
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path(
            "/2.3/networks/network-0001/devices/secondary_wan_config",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(payload.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api
        .set_secondary_wan_config("network-0001", payload)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_device_secondary_wan_access =====================

#[tokio::test]
async fn set_device_secondary_wan_access_deny_true_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb:cc:00:00:01"))
        .and(session_cookie())
        .and(body_json(json!({ "secondary_wan_deny_access": true })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api
        .set_device_secondary_wan_access("network-0001", "aa:bb:cc:00:00:01", true)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn set_device_secondary_wan_access_deny_false_sends_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });

    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aa:bb:cc:00:00:01"))
        .and(session_cookie())
        .and(body_json(json!({ "secondary_wan_deny_access": false })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = wan_api(&mock);
    let env = api
        .set_device_secondary_wan_access("network-0001", "aa:bb:cc:00:00:01", false)
        .await?;

    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== version pins =====================
//
// A network given as a path or an absolute URL keeps the version segment it carries, so each WAN
// call re-pins the resolved URL to 2.3 afterwards (a 2.2 URL would reach a different endpoint).

#[tokio::test]
async fn get_multistaticip_pins_to_2_3_for_every_network_form() -> anyhow::Result<()> {
    for form in NETWORK_FORMS {
        let mock = MockEero::start().await;
        mount_pinned(&mock, "GET", "networks/network-0001/multistaticip", None).await;

        let network = network_form(&mock.uri(), form);
        wan_api(&mock)
            .get_multistaticip(&network, None)
            .await
            .unwrap_or_else(|err| panic!("network form {form}: {err}"));
    }
    Ok(())
}

#[tokio::test]
async fn get_multistaticip_pins_a_published_link_on_any_version_to_2_3() -> anyhow::Result<()> {
    for version in ["2.2", "2.3", "2.4", "10.25"] {
        let mock = MockEero::start().await;
        mount_pinned(&mock, "GET", "networks/network-0001/multistaticip", None).await;

        let link = format!("/{version}/networks/network-0001/multistaticip");
        let parent = json!({"resources": {"multistaticip": link}});
        wan_api(&mock)
            .get_multistaticip("network-0001", Some(&parent))
            .await
            .unwrap_or_else(|err| panic!("published link on {version}: {err}"));
    }
    Ok(())
}

#[tokio::test]
async fn set_multistaticip_pins_v2_3_for_every_network_form() -> anyhow::Result<()> {
    let payload = json!({"enabled": false});
    for form in NETWORK_FORMS {
        let mock = MockEero::start().await;
        mount_pinned(
            &mock,
            "PUT",
            "networks/network-0001/multistaticip",
            Some(payload.clone()),
        )
        .await;

        let network = network_form(&mock.uri(), form);
        wan_api(&mock)
            .set_multistaticip(&network, payload.clone())
            .await
            .unwrap_or_else(|err| panic!("network form {form}: {err}"));
    }
    Ok(())
}

#[tokio::test]
async fn set_secondary_wan_config_pins_v2_3_for_every_network_form() -> anyhow::Result<()> {
    let payload = json!({"devices": []});
    for form in NETWORK_FORMS {
        let mock = MockEero::start().await;
        mount_pinned(
            &mock,
            "PUT",
            "networks/network-0001/devices/secondary_wan_config",
            Some(payload.clone()),
        )
        .await;

        let network = network_form(&mock.uri(), form);
        wan_api(&mock)
            .set_secondary_wan_config(&network, payload.clone())
            .await
            .unwrap_or_else(|err| panic!("network form {form}: {err}"));
    }
    Ok(())
}

#[tokio::test]
async fn set_device_secondary_wan_access_pins_v2_3_for_every_network_form() -> anyhow::Result<()> {
    let body = json!({"secondary_wan_deny_access": true});
    for form in NETWORK_FORMS {
        let mock = MockEero::start().await;
        mount_pinned(
            &mock,
            "PUT",
            "networks/network-0001/devices/aa:bb:cc:00:00:01",
            Some(body.clone()),
        )
        .await;

        let network = network_form(&mock.uri(), form);
        wan_api(&mock)
            .set_device_secondary_wan_access(&network, "aa:bb:cc:00:00:01", true)
            .await
            .unwrap_or_else(|err| panic!("network form {form}: {err}"));
    }
    Ok(())
}

/// `set_device_secondary_wan_access` does not reduce `mac` to its last segment, so a device given
/// as a path or URL carries its own version and is a second route to 2.2 unless it is pinned too.
#[tokio::test]
async fn set_device_secondary_wan_access_pins_v2_3_for_path_form_mac() -> anyhow::Result<()> {
    let body = json!({"secondary_wan_deny_access": true});
    for network_form_name in ["id", "path-2.2", "url-2.2"] {
        for mac_form in ["path", "url"] {
            let mock = MockEero::start().await;
            mount_pinned(
                &mock,
                "PUT",
                "networks/network-0001/devices/aa:bb:cc:00:00:01",
                Some(body.clone()),
            )
            .await;

            let uri = mock.uri();
            let network = network_form(&uri, network_form_name);
            let device_path = "/2.2/networks/network-0001/devices/aa:bb:cc:00:00:01";
            let mac = match mac_form {
                "path" => device_path.to_owned(),
                _ => format!("{uri}{device_path}"),
            };
            wan_api(&mock)
                .set_device_secondary_wan_access(&network, &mac, true)
                .await
                .unwrap_or_else(|err| panic!("network {network_form_name}, mac {mac_form}: {err}"));
        }
    }
    Ok(())
}

/// The pin runs after the network check, so it cannot be used to re-target a device of another
/// network at the addressed one.
#[tokio::test]
async fn set_device_secondary_wan_access_pin_does_not_bypass_the_network_check()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_request(&mock).await;

    let err = wan_api(&mock)
        .set_device_secondary_wan_access(
            "network-0001",
            "/2.2/networks/other-net/devices/aa:bb:cc:00:00:01",
            true,
        )
        .await
        .expect_err("a device of another network must be refused");

    match err {
        Error::Validation { field, message, .. } => {
            assert_eq!(field, "child");
            assert_eq!(message, "must belong to the addressed network");
        }
        other => panic!("expected a validation error, got {other:?}"),
    }
    Ok(())
}

/// Resolve, then pin, then warn, then send: a write that cannot be resolved logs no warning, and
/// one that is sent logs exactly one.
#[tokio::test]
async fn wan_writes_warn_once_after_resolving_and_pinning() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // One authenticated mock per write, each expecting exactly one request on its 2.3 path.
    for (put_path, body) in [
        ("/2.3/networks/network-0001/multistaticip", json!({})),
        (
            "/2.3/networks/network-0001/devices/secondary_wan_config",
            json!({}),
        ),
        (
            "/2.3/networks/network-0001/devices/aa:bb:cc:00:00:01",
            json!({ "secondary_wan_deny_access": true }),
        ),
    ] {
        Mock::given(method("PUT"))
            .and(path(put_path))
            .and(session_cookie())
            .and(user_token_header())
            .and(body_json(body))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&mock.server)
            .await;
    }
    let api = wan_api(&mock);
    let (_guard, logs) = capture_logs();

    api.set_multistaticip("bad/id", json!({}))
        .await
        .expect_err("an invalid network id must be refused");
    api.set_secondary_wan_config("bad/id", json!({}))
        .await
        .expect_err("an invalid network id must be refused");
    api.set_device_secondary_wan_access("bad/id", "aa:bb:cc:00:00:01", true)
        .await
        .expect_err("an invalid network id must be refused");
    assert!(logs.warnings().is_empty(), "got {:?}", logs.warnings());

    api.set_multistaticip("/2.2/networks/network-0001", json!({}))
        .await?;
    assert_eq!(
        logs.count(Level::WARN, "set multi-static-IP configuration for network"),
        1
    );
    api.set_secondary_wan_config("/2.2/networks/network-0001", json!({}))
        .await?;
    assert_eq!(
        logs.count(Level::WARN, "set secondary WAN configuration for network"),
        1
    );
    api.set_device_secondary_wan_access("/2.2/networks/network-0001", "aa:bb:cc:00:00:01", true)
        .await?;
    assert_eq!(
        logs.count(
            Level::WARN,
            "set secondary WAN access for device on network"
        ),
        1
    );
    Ok(())
}

// ===================== URL safety =====================
//
// Every WAN call refuses, before any request, a network or device that could address a different
// resource than the call is meant for. The bare-network-id and device checks are
// `links::validate_identifier`'s; the control-character, not-exactly-one-network and
// published-link checks are `links::resource_url`'s and the published-link rule's caller-path
// rules.

const CONTROLS: [&str; 5] = ["\n", "\t", "\r", " ", "\u{0}"];

/// Which of the four WAN calls a table test drives.
#[derive(Clone, Copy, Debug)]
enum WanCall {
    GetMultistaticip,
    SetMultistaticip,
    SetSecondaryWanConfig,
    SetDeviceSecondaryWanAccess,
}

impl WanCall {
    const ALL: [Self; 4] = [
        Self::GetMultistaticip,
        Self::SetMultistaticip,
        Self::SetSecondaryWanConfig,
        Self::SetDeviceSecondaryWanAccess,
    ];

    async fn run(self, api: &WanApi, network: &str) -> Result<rusteero::envelope::Envelope, Error> {
        match self {
            Self::GetMultistaticip => api.get_multistaticip(network, None).await,
            Self::SetMultistaticip => api.set_multistaticip(network, json!({})).await,
            Self::SetSecondaryWanConfig => api.set_secondary_wan_config(network, json!({})).await,
            Self::SetDeviceSecondaryWanAccess => {
                api.set_device_secondary_wan_access(network, "aa:bb:cc:00:00:01", true)
                    .await
            }
        }
    }
}

#[tokio::test]
async fn a_control_character_in_the_network_is_refused_before_any_request() -> anyhow::Result<()> {
    for call in WanCall::ALL {
        for control in CONTROLS {
            let mock = MockEero::start().await;
            expect_no_request(&mock).await;
            let uri = mock.uri();
            let upper = uri.replacen("http", "HTTP", 1);

            for network in [
                format!("net{control}work_123"),
                format!("/2.2/networks/network_123{control}"),
                format!("{uri}/2.2/networks/network_123{control}"),
                format!("{upper}/2.2/networks/net{control}work_123"),
            ] {
                let err = call
                    .run(&wan_api(&mock), &network)
                    .await
                    .expect_err("a control character in the network must be refused");
                assert!(
                    matches!(err, Error::Validation { .. }),
                    "{call:?} {network:?} gave {err:?}"
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_network_path_that_is_not_exactly_one_network_is_refused() -> anyhow::Result<()> {
    for call in WanCall::ALL {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;

        for network in [
            "/2.2/account",
            "/2.2/networks/network_123/../../account",
            "/2.2/networks/network_123?x=1",
            "/2.2/networks/%2e%2e/account",
        ] {
            let err = call
                .run(&wan_api(&mock), network)
                .await
                .expect_err("a path that is not exactly one network must be refused");
            assert!(
                matches!(err, Error::Validation { .. }),
                "{call:?} {network:?} gave {err:?}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_published_multistaticip_link_with_a_control_character_is_refused() -> anyhow::Result<()>
{
    for control in ["\n", "\t", "\r", "\u{0}"] {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;

        let link = format!("/2.2/networks/network_123/multi{control}");
        let parent = json!({"resources": {"multistaticip": link}});
        let err = wan_api(&mock)
            .get_multistaticip("network_123", Some(&parent))
            .await
            .expect_err("a published link carrying a control character must be refused");
        assert!(
            matches!(err, Error::Validation { ref field, .. } if field == "link"),
            "{control:?} gave {err:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_unsafe_device_mac_is_refused_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_request(&mock).await;

    for mac in [
        "AA:BB:CC:DD:EE:FF\n",
        "AA:BB:CC\t:DD:EE:FF",
        "..",
        "AA:BB:CC:DD:EE:FF?x=1",
        "",
    ] {
        let err = wan_api(&mock)
            .set_device_secondary_wan_access("network_123", mac, true)
            .await
            .expect_err("an unsafe mac must be refused");
        assert!(
            matches!(err, Error::Validation { .. }),
            "{mac:?} gave {err:?}"
        );
    }
    Ok(())
}
