//! HTTP integration tests for `EerosApi`'s read (`GET`) methods, pinned against
//! `eero-api src/eero/api/eeros.py`. See `src/endpoints/eeros.rs`'s module docs for the
//! `network_id`-parameter divergence these tests exercise indirectly (by never passing one to
//! `get_eero`/`get_led_status`/`get_nightlight`).

mod common;

use std::sync::Arc;

use rusteero::endpoints::eeros::EerosApi;
use rusteero::error::Error;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds an `EerosApi` pointed at `mock`, authenticated with [`TEST_TOKEN`].
fn eeros_api(mock: &MockEero) -> EerosApi {
    EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_eeros =====================

#[tokio::test]
async fn get_eeros_hits_the_networks_nested_path_with_the_session_cookie() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_eeros("network-0001").await?;

    assert_eq!(env.into_value(), fixture_json("eeros.json"));
    Ok(())
}

// ===================== get_eero =====================

// This is the test most likely to be wrong per the task brief: `get_eero` must NOT be nested
// under `networks/`, unlike `get_eeros` above. The exact path is asserted explicitly (both via
// the wiremock matcher, which only responds on that literal path, and via a second, independent
// full-URL assertion below) so a regression to `networks/{id}/eeros/{eero_id}` cannot pass
// silently.
#[tokio::test]
async fn get_eero_hits_the_top_level_eeros_path_not_nested_under_networks() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_eero("eero-0001").await?;

    // Independent confirmation the rendered URL is exactly what was asserted via the wiremock
    // path matcher above — belt and braces for the one route this task brief calls out as most
    // likely to regress.
    let rendered = rusteero::routes::GET_EERO.render(&[("eero_id", "eero-0001")])?;
    assert_eq!(rendered.path(), "/2.2/eeros/eero-0001");

    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}

// ===================== get_led_status =====================

#[tokio::test]
async fn get_led_status_hits_the_same_path_as_get_eero() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_led_status("eero-0001").await?;

    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}

// ===================== get_nightlight =====================

#[tokio::test]
async fn get_nightlight_hits_the_same_path_as_get_eero() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let env = api.get_nightlight("eero-0001").await?;

    assert_eq!(env.into_value(), fixture_json("eero.json"));
    Ok(())
}

// ===================== 404 mapping =====================

#[tokio::test]
async fn get_eero_404_maps_to_error_api_with_status_404() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/eeros/missing-eero"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such eero"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eeros_api(&mock);
    let err = api
        .get_eero("missing-eero")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    Ok(())
}
