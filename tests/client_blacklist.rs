//! `Client` integration suite for `BlacklistApi`'s wrappers at `v8.4.1`: `get_blacklist`, and the
//! direct `add_to_blacklist`/`remove_from_blacklist` (`client.py:1899-1960`), which call the
//! blacklist domain methods themselves — never the `DevicesAPI` block/unblock aliases that
//! `tests/client_devices.rs` covers through `Client::block_device`/`Client::unblock_device`.

mod common;

use serde_json::{Value, json};
use tracing::Level;
use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;

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
async fn get_blacklist_hits_the_blacklist_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_blacklist(Some("network-0001")).await?;
    Ok(())
}

/// The colon-separated MAC the add tests block, and the colon-stripped id the block list reports.
const MAC: &str = "aa:bb:cc:11:22:33";
const DEVICE_ID: &str = "aabbcc112233";

/// A network envelope that publishes `device_blacklist` at API 2.4.
fn network_with_blacklist_link() -> Value {
    json!({
        "meta": {"code": 200},
        "data": {
            "id": "network-0001",
            "url": "/2.2/networks/network-0001",
            "resources": {
                "device_blacklist": "/2.4/networks/network-0001/blacklist",
            },
        },
    })
}

/// Mounts the authenticated form-encoded `POST <post_path>` of [`MAC`], expecting `calls`.
async fn mount_add_post(mock: &MockEero, post_path: &str, calls: u64) {
    Mock::given(method("POST"))
        .and(path(post_path))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("mac=aa%3Abb%3Acc%3A11%3A22%3A33"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {"code": 200}})))
        .expect(calls)
        .mount(&mock.server)
        .await;
}

/// Mounts the authenticated `DELETE <delete_path>`, expecting `calls`.
async fn mount_delete(mock: &MockEero, delete_path: &str, calls: u64) {
    Mock::given(method("DELETE"))
        .and(path(delete_path))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"meta": {"code": 200}})))
        .expect(calls)
        .mount(&mock.server)
        .await;
}

/// Mounts a trap on `verb path` that must never be reached.
async fn mount_trap(mock: &MockEero, verb: &str, trap_path: &str) {
    Mock::given(method(verb))
        .and(path(trap_path))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
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

/// The requests the mock has received, as `VERB path` strings.
async fn received(mock: &MockEero) -> Vec<String> {
    mock.server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect()
}

#[tokio::test]
async fn add_to_blacklist_posts_the_form_mac_once() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_add_post(&mock, "/2.2/networks/network-0001/blacklist", 1).await;

    let client = client(&mock).await;
    let env = client
        .add_to_blacklist(MAC, Some("network-0001"), None)
        .await?;

    assert_eq!(env.into_value(), json!({"meta": {"code": 200}}));
    // Only the blacklist write: no discovery, no parent GET, and never the `/devices` block path.
    assert_eq!(
        received(&mock).await,
        vec!["POST /2.2/networks/network-0001/blacklist"]
    );
    Ok(())
}

#[tokio::test]
async fn remove_from_blacklist_deletes_the_child_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_delete(
        &mock,
        "/2.2/networks/network-0001/blacklist/aabbcc112233",
        1,
    )
    .await;

    let client = client(&mock).await;
    let env = client
        .remove_from_blacklist(DEVICE_ID, Some("network-0001"), None)
        .await?;

    assert_eq!(env.into_value(), json!({"meta": {"code": 200}}));
    assert_eq!(
        received(&mock).await,
        vec!["DELETE /2.2/networks/network-0001/blacklist/aabbcc112233"]
    );
    Ok(())
}

#[tokio::test]
async fn add_to_blacklist_logs_no_uncharacterised_warning() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_add_post(&mock, "/2.2/networks/network-0001/blacklist", 1).await;

    let client = client(&mock).await;
    let (_guard, logs) = common::capture_logs();
    client
        .add_to_blacklist(MAC, Some("network-0001"), None)
        .await?;

    assert!(
        logs.warnings().is_empty(),
        "unexpected warnings: {:?}",
        logs.warnings()
    );
    // Positive control: the capture does see such a warning when one is emitted.
    rusteero::links::warn_uncharacterised_write("probe");
    assert_eq!(logs.count(Level::WARN, "not been fully characterised"), 1);
    Ok(())
}

/// Both wrappers drop the device's own cache entry and the device list, and nothing else: another
/// device, the network entry and the other network's list stay cached.
#[tokio::test]
async fn both_invalidate_the_device_entry_and_list_but_not_the_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_add_post(&mock, "/2.2/networks/network-0001/blacklist", 1).await;
    mount_delete(
        &mock,
        "/2.2/networks/network-0001/blacklist/aabbcc112233",
        1,
    )
    .await;
    // The list is read before, between and after the writes: 3 requests for 2 invalidations.
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices",
        fixture("devices.json"),
        3,
    )
    .await;
    // `MAC` and `DEVICE_ID` are two spellings of one device, so they share one canonical cache
    // entry: it is fetched once before the writes and once after each write, under whichever
    // spelling asks first.
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/aa:bb:cc:11:22:33",
        fixture("device.json"),
        2,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/aabbcc112233",
        fixture("device.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/device-0002",
        fixture("device.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0002/devices",
        fixture("devices.json"),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        fixture("network.json"),
        1,
    )
    .await;

    let client = client(&mock).await;
    let nid = Some("network-0001");
    let prime = async |client: &Client| -> Result<(), Error> {
        client.get_devices(nid, false, None, None).await?;
        client.get_device("device-0002", nid, false).await?;
        client
            .get_devices(Some("network-0002"), false, None, None)
            .await?;
        client.get_network(nid, false).await?;
        Ok(())
    };
    prime(&client).await?;
    client.get_device(MAC, nid, false).await?;
    // Same device, other spelling: served from the entry `MAC` just filled.
    client.get_device(DEVICE_ID, nid, false).await?;

    client.add_to_blacklist(MAC, nid, None).await?;
    // Dropped: the list and the device's canonical entry (each a fresh request) ...
    client.get_devices(nid, false, None, None).await?;
    client.get_device(MAC, nid, false).await?;
    client.get_device(DEVICE_ID, nid, false).await?;
    // ... kept: the bystander, the other network's list and the network.
    prime(&client).await?;

    // Removing by the other spelling drops the same canonical entry.
    client.remove_from_blacklist(DEVICE_ID, nid, None).await?;
    client.get_devices(nid, false, None, None).await?;
    client.get_device(DEVICE_ID, nid, false).await?;
    client.get_device(MAC, nid, false).await?;
    prime(&client).await?;
    Ok(())
}

#[tokio::test]
async fn add_to_blacklist_prefers_the_callers_parent_over_the_cached_networks_link()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        network_with_blacklist_link().to_string(),
        1,
    )
    .await;
    // The caller's link is a third, distinct path.
    mount_add_post(&mock, "/2.3/networks/network-0001/blacklist", 1).await;
    mount_trap(&mock, "POST", "/2.4/networks/network-0001/blacklist").await;
    mount_trap(&mock, "POST", "/2.2/networks/network-0001/blacklist").await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let parent = json!({"resources": {"device_blacklist": "/2.3/networks/network-0001/blacklist"}});
    client
        .add_to_blacklist(MAC, Some("network-0001"), Some(&parent))
        .await?;
    Ok(())
}

#[tokio::test]
async fn both_use_the_cached_networks_link_without_a_caller_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001",
        network_with_blacklist_link().to_string(),
        1,
    )
    .await;
    mount_add_post(&mock, "/2.4/networks/network-0001/blacklist", 1).await;
    mount_delete(
        &mock,
        "/2.4/networks/network-0001/blacklist/aabbcc112233",
        1,
    )
    .await;
    mount_trap(&mock, "POST", "/2.2/networks/network-0001/blacklist").await;
    mount_trap(
        &mock,
        "DELETE",
        "/2.2/networks/network-0001/blacklist/aabbcc112233",
    )
    .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .add_to_blacklist(MAC, Some("network-0001"), None)
        .await?;
    client
        .remove_from_blacklist(DEVICE_ID, Some("network-0001"), None)
        .await?;
    Ok(())
}

/// An empty MAC is rejected before the write (Python sends the POST and only then raises from its
/// cache invalidation), so nothing is sent and the cached device list is untouched.
#[tokio::test]
async fn add_to_blacklist_rejects_an_empty_mac_before_the_write() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_trap(&mock, "POST", "/2.2/networks/network-0001/blacklist").await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices",
        fixture("devices.json"),
        1,
    )
    .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    let err = client
        .add_to_blacklist("", Some("network-0001"), None)
        .await
        .expect_err("an empty MAC must be rejected before the write");
    assert!(
        matches!(
            &err,
            Error::Validation { field, message, .. }
                if field == "id_or_url" && message == "must be a non-empty string"
        ),
        "{err:?}"
    );
    // Still cached: the mock allows only the first list request.
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    assert_eq!(
        received(&mock).await,
        vec!["GET /2.2/networks/network-0001/devices"]
    );
    Ok(())
}

#[tokio::test]
async fn remove_from_blacklist_rejects_an_empty_id_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    let client = client(&mock).await;
    let err = client
        .remove_from_blacklist("", Some("network-0001"), None)
        .await
        .expect_err("an empty id must be rejected before any request");
    assert!(
        matches!(&err, Error::Validation { field, .. } if field == "id"),
        "{err:?}"
    );
    assert!(received(&mock).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_failed_write_leaves_the_device_cache_alone() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock.server)
        .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices",
        fixture("devices.json"),
        1,
    )
    .await;

    let client = client(&mock).await;
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    client
        .add_to_blacklist(MAC, Some("network-0001"), None)
        .await
        .expect_err("a server error must propagate");
    client
        .get_devices(Some("network-0001"), false, None, None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn both_resolve_the_network_without_auto_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(&mock, "/2.2/networks", "{}".to_owned(), 0).await;

    let client = client(&mock).await;
    let err = client
        .add_to_blacklist(MAC, None, None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    let err = client
        .remove_from_blacklist(DEVICE_ID, None, None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    assert!(received(&mock).await.is_empty());
    Ok(())
}
