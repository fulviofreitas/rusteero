//! `Client` integration suite for the `AccountAPI` domain (new in v8.0.0), against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! Every write here is unverified against a live account (see `src/endpoints/account.rs`); these
//! tests pin the request shape and the `account`-bucket cache-invalidation contract from
//! `.claude/tasks/briefs/v8/client.md` §4 ("account (`AccountAPI`...)"), not live behaviour.

mod common;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::cache::CacheKey;
use rusteero::client::Client;

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

// ===================== set_account_name resets the account cache =====================

#[tokio::test]
async fn set_account_name_resets_the_account_cache_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/name"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.set_account_name("New Name").await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== set_account_email is NOT invalidated =====================

#[tokio::test]
async fn set_account_email_does_not_invalidate_the_account_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/email"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.set_account_email("user@example.com").await?;
    // Second read is served from cache: only one GET to `/account` total.
    client.get_account(false).await?;
    Ok(())
}

// ===================== verify_account_email resets the account cache =====================

#[tokio::test]
async fn verify_account_email_resets_the_account_cache_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/email/verify"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.verify_account_email("123456").await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== set_account_phone is NOT invalidated =====================

#[tokio::test]
async fn set_account_phone_does_not_invalidate_the_account_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/phone"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.set_account_phone("+15555550100").await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== verify_account_phone resets the account cache =====================

#[tokio::test]
async fn verify_account_phone_resets_the_account_cache_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/account/phone/verify"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.verify_account_phone("654321").await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== set_account_consents resets the account cache =====================

#[tokio::test]
async fn set_account_consents_resets_the_account_cache_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/account/consents"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.set_account_consents(true).await?;
    client.get_account(false).await?;
    Ok(())
}

// ===================== get_sms_countries: never cached, no network_id =====================

#[tokio::test]
async fn get_sms_countries_is_never_cached() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/countries/sms"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"meta":{"code":200},"data":{"countries":["US"]}})),
        )
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_sms_countries().await?;
    client.get_sms_countries().await?;
    Ok(())
}

// ===================== unit: CacheKey::Account round-trips through invalidate =====================

#[test]
fn cache_key_account_is_a_unit_variant() {
    // Compile-time/API-shape smoke test: `CacheKey::Account` must remain a bare unit variant
    // (no id parameter) for `set_account_name`/`verify_account_email`/`verify_account_phone`/
    // `set_account_consents` to keep invalidating it with `&CacheKey::Account`. A `const` (rather
    // than a `let` binding) keeps this a pure compile-time check with no runtime no-op binding.
    const _CHECK: CacheKey = CacheKey::Account;
}
