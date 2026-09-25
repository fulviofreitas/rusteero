//! `Client` integration suite for `ScheduleApi`'s cache invalidation (security review finding
//! F3).

mod common;

use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

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

/// **F3**: `enable_bedtime` delegates to `ScheduleApi::set_profile_schedule` — the same `PUT
/// .../profiles/{pid}` that `Client::set_profile_schedule` invalidates the profile cache for.
/// Before the fix this was a faithful-to-Python no-op; the fix makes it match its own delegate.
#[tokio::test]
async fn enable_bedtime_invalidates_the_profile_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .enable_bedtime("profile-0001", "21:00", "07:00", None, Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

/// **F3**: `clear_profile_schedule` must invalidate the profile cache too — see
/// `enable_bedtime_invalidates_the_profile_bucket` above.
#[tokio::test]
async fn clear_profile_schedule_invalidates_the_profile_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .clear_profile_schedule("profile-0001", Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

/// **F3**: `set_weekday_bedtime` must invalidate the profile cache too — see
/// `enable_bedtime_invalidates_the_profile_bucket` above.
#[tokio::test]
async fn set_weekday_bedtime_invalidates_the_profile_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .set_weekday_bedtime("profile-0001", "21:00", "07:00", Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

/// **F3**: `set_weekend_bedtime` must invalidate the profile cache too — see
/// `enable_bedtime_invalidates_the_profile_bucket` above.
#[tokio::test]
async fn set_weekend_bedtime_invalidates_the_profile_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .set_weekend_bedtime("profile-0001", "21:00", "07:00", Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}
