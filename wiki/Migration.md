# 🔀 Migration

Upgrade guide between `rusteero` major releases. The crate tracks the Python
[`eero-api`](https://github.com/fulviofreitas/eero-api) library, so each `rusteero` major maps
onto an `eero-api` release: 1.0.0 ported `eero-api` 6.2.0, 2.0.0 ported 8.0.4, and 3.0.0 ports
8.4.1. Where a change below has a Python counterpart, the Python
[Migration](https://github.com/fulviofreitas/eero-api/wiki/Migration) page (v6.x → v7.0.0 and
v7.x → v8.0.0) and its [Deprecations](https://github.com/fulviofreitas/eero-api/wiki/Deprecations)
register give the wire-level reasoning.

---

## 2.x → 3.0.0 (eero-api 8.4.1)

**What broke**: four signature changes, listed first; the compiler finds every one of them
(with one caveat for the async setters, see [3](#3-the-session-setters-are-async)). Everything
else in this release is either new surface or a change of behaviour behind an unchanged
signature, which the compiler cannot point you to, so read
[Behaviour changes](#behaviour-changes) as well.

### Breaking changes

#### 1. `Error::Authentication` and `Error::RateLimit` carry `status`

Both variants gained `status: Option<u16>`. A server-driven `Authentication` carries
`Some(401)`, including a 401 whose `meta.error` is `error.rate.limit`. A `RateLimit` carries the
response's own status: `Some(429)`, or for example `Some(400)` or `Some(503)` when
`error.rate.limit` arrives on another status. The local `"Not authenticated"` guard and the
`"Login failed: …"` / `"Verification failed: …"` errors that `LoginFlow::start` and
`PendingLogin::verify` build around another failure carry `None`; a real 401 from login or verify
keeps `Some(401)`. A struct literal or a pattern that lists every field no longer compiles: add
`status`, or end the pattern with `..`.

```rust
// Before (2.x)
match err {
    Error::Authentication { message, envelope, error_code } => { /* … */ }
    Error::RateLimit { message, retry_after, envelope, error_code } => { /* … */ }
    _ => {}
}
let local = Error::Authentication { message: "no token".into(), envelope: None, error_code: None };

// After (3.0.0)
match err {
    Error::Authentication { error_code, status, .. } => { /* status: Some(401), or None if local */ }
    Error::RateLimit { retry_after, status, .. } => { /* status: Some(429), Some(503), … */ }
    _ => {}
}
let local = Error::authentication("no token");                 // status: None
```

`Error::status()` returns the new field, so it is now `Some(401)` / `Some(429)` (or whatever the
response carried) for these two variants, where 2.x returned `None`. `is_auth_error()` is
unchanged in effect: `true` for every `Authentication`, `false` for every `RateLimit` whatever its
status.

#### 2. The data-usage reads take `timeout: Option<RequestTimeout>`

The nine time-windowed data-usage reads gained a `timeout` parameter, immediately before
`network_id` on `Client` and immediately before `parent` on `DataUsageApi`:

| `Client` | `client.api().data_usage()` |
|---|---|
| `get_data_usage` | `get_data_usage` |
| `get_data_usage_breakdown` | `get_breakdown` |
| `get_devices_data_usage` | `get_devices_usage` |
| `get_device_data_usage` | `get_device_usage` |
| `get_eeros_data_usage_summary` | `get_eeros_summary` |
| `get_eero_data_usage` | `get_eero_usage` |
| `get_profile_data_usage` | `get_profile_usage` |
| `get_unprofiled_devices_data_usage` | `get_unprofiled_devices` |
| `get_unprofiled_data_usage_summary` | `get_unprofiled_summary` |

```rust
// Before (2.x)
client.get_data_usage(start, end, "daily", None, None).await?;
client.get_device_data_usage(mac, start, end, "hourly", None, None).await?;
api.data_usage().get_data_usage(&nid, start, end, "daily", None, None).await?;

// After (3.0.0): None keeps the transport default (30 s total, 10 s per read)
client.get_data_usage(start, end, "daily", None, None, None).await?;
client.get_device_data_usage(mac, start, end, "hourly", None, None, None).await?;
api.data_usage().get_data_usage(&nid, start, end, "daily", None, None, None).await?;

// Or give a long hourly window more time
use rusteero::RequestTimeout;
use std::time::Duration;

let slow = RequestTimeout::new(Some(Duration::from_secs(120)), Some(Duration::from_secs(30)));
client.get_data_usage(start, end, "hourly", None, Some(slow), None).await?;
```

`Some(t)` replaces **both** bounds for that one request: a field left `None`, or set to
`Duration::ZERO`, is unbounded and never falls back to the default, so
`RequestTimeout::total(d)` has no read bound at all. The new `get_eeros_data_usage` /
`get_eeros_usage` takes `timeout` in the same place. `get_data_usage_report_settings` and
`set_data_usage_report_settings` are unchanged. See
[Configuration — Transport options](Configuration#transport-options) for the full semantics.

#### 3. The session setters are async

`AuthApi::{set_session_token, clear_session_token, clear_auth_data}` and
`Client::{set_session_token, clear_session_token}` are now `async fn`:

```rust
// Before (2.x)
client.set_session_token("<token>")?;
client.clear_session_token()?;
client.api().auth().clear_auth_data()?;

// After (3.0.0)
client.set_session_token("<token>").await?;
client.clear_session_token().await?;
client.api().auth().clear_auth_data().await?;
client.clear_auth_data().await?;          // new on Client; also drops an explicit preferred network
```

The caveat: `client.set_session_token("<token>")?` stops compiling, but a bare statement such as
`client.clear_session_token();` (no `?`, no `.await`) still compiles, with only an
`unused_must_use` warning, and **does nothing**. Build with `-D warnings`, or grep for the five
names.

Each call replaces the in-memory session before its first `.await` yields, so every other task
sees the change at once, then runs the credential-store write on Tokio's blocking pool, where
dropping the future no longer cancels it. When several changes overlap, the last one applied wins
in memory and in the store. They need a Tokio runtime (current-thread or multi-thread); from
synchronous code, drive them with your runtime's `block_on`. `Transport::set_session` stays
synchronous and blocks on the store; `Transport::set_session_async` is its async counterpart.

#### 4. `create_burst_reporter`'s `reporter_data` is `Option<Value>`, and deprecated

```rust
// Before (2.x)
client.api().burst_reporters().create_burst_reporter(&nid, json!({}), None).await?;

// After (3.0.0)
client.api().burst_reporters().create_burst_reporter(&nid, None, None).await?;   // sends ""
client.create_burst_reporter(None, None).await?;                                  // new Client wrapper
```

`None` (or `Some(Value::Null)`) sends the two-byte JSON string `""`, the body that was verified
against a live network, and logs nothing. Any other `Some(..)` first logs a deprecation `WARN`.
A falsy value (`false`, `0`, `""`, `[]`, `{}`) is then ignored and `""` is sent; a truthy one is
still sent as the JSON body, with an uncharacterised-write warning. The parameter goes away when
`eero-api` 9.0.0 removes it. Rust cannot mark a single parameter `#[deprecated]`, so the compiler
does not flag a `Some(..)`: grep for it.

### Behaviour changes

These keep their signatures, so they compile unchanged and behave differently.

**Requests and responses**

* **New default `User-Agent`**: `eero-ios/2026.8.1 (iPhone; iOS 18.5)`
  (`consts::DEFAULT_USER_AGENT`), replacing `eero/3.0 (iPhone; iOS 17.0)`, for every request,
  login and refresh included. The API reads it as the app version and advertises more
  capabilities to the new string (`led_action`, `device_blacklist`, `diagnostics`,
  `device_usage`, `block_apps`, `ddns_enabled`, `historical_usage`, the `dnsfilter_*` entries and
  more were missing under the old one). Under it, the network's `resources` map also points
  `forwards`, `routing` and `support` at API 2.3; the three reads are pinned to 2.2 (below), so
  their shape does not change. To keep the previous string:

  ```rust
  use rusteero::consts::LEGACY_USER_AGENT;

  let client = Client::builder()
      .session(Some(session))
      .user_agent(Some(LEGACY_USER_AGENT.to_owned()))
      .build()
      .await?;
  ```

  `LoginFlow::new(..)` sends the default; `LoginFlow::with_transport(transport)` sends whatever
  that transport was built with. See [Configuration — The `User-Agent`](Configuration#the-user-agent).
* **Version pins.** `get_support`, `get_forwards` and `get_routing` (domain and `Client`) always
  go to API 2.2, even when a `parent` envelope publishes a 2.3 link: on 2.3, `support` returns a
  `contacts` array instead of a flat object. `set_device_nickname` and `pause_device` always go
  to 2.3, and so do the WAN methods (`get_multistaticip`, `set_multistaticip`,
  `set_secondary_wan_config`, `set_device_secondary_wan_access`), whatever form the network
  takes: a `/2.2/networks/<id>` path or a 2.2 URL no longer carries its version into the request.
  `get_multistaticip` is re-pinned even when it follows a published link.
* **A `2xx` body that is valid JSON but not an object** (a list, a string, a number, `true`,
  `null`) is now `Error::Api` with `Unexpected JSON response: expected an object`, where 2.x
  returned it wrapped in an `Envelope`. A `2xx` body that is not UTF-8 is `Error::Api` with
  `Response body is not valid UTF-8`. A non-`2xx` body is decoded lossily, so a stray invalid
  byte no longer hides its status.
* **`Retry-After` is capped at 24 hours** (`consts::RETRY_AFTER_MAX`) before it reaches
  `Error::RateLimit`'s `retry_after`. Delta-seconds must be plain ASCII digits (a leading `+` is
  no longer accepted), and an HTTP date may now also be in RFC 850 or asctime form, not only an
  IMF-fixdate.
* **`get_retries` follows the classified error**: it retries `Error::Network`, `Error::Timeout`
  and any error whose status is `500` or more, except a rate limit on any status (a `5xx`
  carrying `error.rate.limit` is no longer retried).
* **`TransportBuilder::timeout(Duration::ZERO)`** and **`read_timeout(Duration::ZERO)`** now
  switch that bound off (unbounded), where 2.x timed out at once. The crate now enforces both
  bounds itself: the read bound also covers connecting, writing the request and waiting for the
  response head.
* **`set_thread_enabled`** is now an alias of `update_thread(Some(enabled), None, ..)`: it sends
  `{"thread_enable": enabled}` instead of `{"enabled": enabled}`, still as a JSON `PUT` to
  `networks/{id}/thread`, and logs `update_thread`'s warning. It remains unverified.
* **`set_dhcp(Some("manual"), ..)`** is sent as `"custom"`, the value the API accepts (it refuses a
  literal `"manual"` with `400`); `"custom"` is now accepted as a mode too. Every DHCP write is
  confirmed to restart the whole mesh, about eight minutes of outage.
* **`add_to_blacklist`, `block_device` and `create_burst_reporter`** (with the `""` body) were
  verified against a live network and no longer log the uncharacterised-write warning.

**Validation, before anything is sent**

* **Empty names and passwords are rejected.** `set_network_name`, `set_network_password`,
  `set_guest_password` and `set_guest_network(.., Some(""), ..)` given `""` return
  `Error::Validation { field: "name" | "password", message: "must be a non-empty string" }`,
  on `Client` before the network is resolved or discovered. 2.x sent the request.
* **Changed `Validation` field names.** Match on these if you inspect `field`:

  | Check | 2.x `field` | 3.0.0 `field` |
  |---|---|---|
  | A path you pass as a resource argument (`/2.2/networks/<id>`) | `"link"` | `"id_or_url"` (a link read from a `parent` envelope keeps `"link"`) |
  | The `User-Agent` builder value | `"user_agent"` | `"User-Agent"`, checked after `"X-Accept-Language"` |
  | `base_url` | `"base_url"`, message `not a valid URL: <parser text>` | `"base_url"`, with one of the fixed messages `must be an absolute URL with a scheme and a host`, `must not carry userinfo` or `has an authority that cannot be parsed`; `""` is rejected |
  | An absolute URL off the API origin | `"url"`, `must be an absolute <scheme> URL on <host>` | `"url"`, `must be an absolute <scheme> URL on <host> with no userinfo and no other port` |
  | A request URL with userinfo passed to `Transport::request` | sent | `"url"`, `must not carry userinfo` (reqwest would send it as Basic credentials) |

  `ClientBuilder::build()` now checks `base_url`, `X-Accept-Language` and `User-Agent`, in that
  order, before it reads a credential store. See
  [Troubleshooting](Troubleshooting#errorvalidation-before-any-request-was-sent) for every field.
* **Stricter paths and URLs.** A path or absolute URL you pass must now name exactly one
  resource of the family the method addresses (`/<version>/networks/<id>` where a network is
  expected), so `/2.2/account` passed as a network, or a network path passed to `reboot_eero`, is
  `must be a path under <family> on the API host`. It must also be a plain path: no whitespace or
  control character, no query or fragment, no `.`/`..` segment or percent-encoded dot, no
  backslash. An absolute URL must be on the API host's full origin: same scheme, host and
  *effective* port, and no userinfo at all. Bare ids are unaffected; a link read from a `parent`
  envelope gets the plain-path rule but is never confined to a family or a version. See
  [Rust API — Ids, paths, URLs and `parent`](Rust-API#ids-paths-urls-and-parent).
* **`update_schedule` and `delete_schedule` refuse a bare schedule id**
  (`field: "schedule"`): a pause lives under a network and a profile these methods are not
  given. Pass the pause's own path or URL (exactly
  `/<version>/networks/{id}/profiles/{id}/schedules/{id}`) or its envelope as `parent`.

**Sessions, storage and the cache**

* **Verified saves.** `FileStore`, `KeyringStore` and `ChainedStore` read the record back after
  every `save` and return `Ok(())` only if the token is there. A write the backend accepted but
  did not keep is the new `StorageError::NotRetained { backend }`, so under
  `StorageFailures::Fatal` a keyring that silently drops writes is now an `Error::Storage` where
  2.x succeeded. Under the default `Warn`, check `credentials_persisted()`. `ChainedStore` falls
  back unless the primary write is verified, retires a fallback record only when it is its own,
  and logs a fixed `WARN` when `clear` leaves a backend uncleared. See
  [Configuration — Credential stores](Configuration#credential-stores).
* **A session change resets the `Client`.** `set_session_token`, `clear_session_token` and
  `logout` empty the cache **and forget a preferred network that auto-discovery chose**, so the
  next call without a `network_id` rediscovers the new account's network. A network set with
  `set_preferred_network` survives them; the new `Client::clear_auth_data` drops it too.
* **Cache details.** A cached getter whose request was overtaken by an invalidation no longer
  writes the pre-write answer back; a device's entry is keyed by its canonical MAC, so any
  spelling hits it; expired entries are dropped when read. See [Rust API — Caching](Rust-API#caching).

**Logging and deprecations**

* **No transport `WARN` or `ERROR` line carries a request URL, a path or a query string**, and their
  wording follows `eero-api` (`Request timed out on GET request`, `Network error (connect) on GET
  request`, `Retrying GET request after transient failure (attempt 2/2)`, …). The path appears
  at `DEBUG` only. Update any alerting that matched the old text. See
  [Configuration — Logging and redaction](Configuration#logging-and-redaction).
* **`redact_sensitive` walks a top-level array** and replaces anything nested 16 levels deep or
  more with `[REDACTED:cyclic-or-too-deep]`.
* **`NetworksApi::get_account` is deprecated** in favour of `AccountApi::get_account`
  (`client.api().account().get_account()`); the wire request is the same `GET /2.2/account`.
  `Client::get_account` and the `get_networks()` fallback already use the new one.

### New surface

* **`Client` wrappers** (each with the domain method behind it):

  | `Client` method | Request |
  |---|---|
  | `get_eeros_data_usage(start, end, cadence, timezone, timeout, nid)` | `GET networks/{id}/data_usage/eeros` |
  | `add_to_blacklist(mac, nid, parent)` | `POST networks/{id}/blacklist`, form `mac=<mac>` (verified) |
  | `remove_from_blacklist(mac_or_device_id, nid, parent)` | `DELETE networks/{id}/blacklist/{mac}` (verified) |
  | `create_burst_reporter(nid, parent)` | `POST networks/{id}/burst_reporters`, body `""` (verified) |
  | `reboot_network(nid, parent)` | `POST networks/{id}/reboot`, body `""`; restarts every eero |
  | `set_hide_5g(enabled, nid)` | `PUT {"value": true}` or a body-less `DELETE` to `networks/{id}/temporary_flags/hide_5g` |
  | `set_preferred_update_hour(hour, nid)` | `POST networks/{id}/updates/preferred_update_hour`, `{"preferred_update_hour": hour}`, `hour` in `0..=23` |
  | `set_ipv6_upstream(enabled, nid, parent)` | `PUT {"ipv6_upstream": enabled}` to the network's settings link |
  | `set_network_ad_block(enabled, profiles, nid, parent)` | `POST networks/{id}/dns_policies/adblock`, `{"enable": enabled}` plus `"profiles"` when given (premium) |
  | `set_network_block_malware(enabled, nid, parent)` | `POST networks/{id}/dns_policies/network`, `{"block_malware": enabled}` (premium) |
  | `request_support(request_data, nid, parent)` | JSON `POST` of `request_data` to the network's support link |
  | `set_weekday_bedtime(profile_id, start_time, end_time, nid, parent)` / `set_weekend_bedtime(..)` | Create one Bedtime pause on the profile, Monday to Friday / Saturday and Sunday |
  | `clear_auth_data()` | No request: clears the session and the store, the cache and any preferred network |
  | `credentials_persisted()` | No request: whether the most recent save reached the store and was kept |

  None of these auto-discovers the network. The wrappers that take `parent` forward yours, and
  otherwise pass the cached network envelope (the profile's envelope for the bedtime helpers,
  with no cache fallback).
* **Domain methods**: `AccountApi::get_account` (`GET /2.2/account`),
  `NetworksApi::set_hide_5g(network_id, enabled)`,
  `UpdatesApi::set_preferred_update_hour(network_id, hour)`,
  `SecurityApi::set_ipv6_upstream(network_id, enabled, parent)`,
  `DnsPoliciesApi::set_network_ad_block(network_id, enabled, profiles, parent)` and
  `set_network_block_malware(network_id, enabled, parent)`,
  `DataUsageApi::get_eeros_usage(network_id, start, end, cadence, timezone, timeout, parent)`.
  Only `add_to_blacklist`, `remove_from_blacklist` and `create_burst_reporter` among the writes
  are verified; the rest log the uncharacterised-write warning.
* **Timeouts**: `rusteero::RequestTimeout { total, read }` (`new`, `total`, `Default`) and
  `Transport::{request, resource, nested}_with_timeout`.
* **Sessions and storage**: `credentials_persisted()` on `Client`, `EeroApi`, `AuthApi` and
  `Transport`; `Transport::set_session_async`; `StorageError::NotRetained { backend }`;
  `KeyringStore::warn_on_unpersisted(bool)`; the public
  `rusteero::storage::{load_async, save_async, clear_async}` adapters.
* **Links and validation**: `rewrite_version` (also at the crate root),
  `links::require_family_path`, `links::origin_mismatch` with `OriginMismatch` and the
  `ORIGIN_*` reason constants, `params::require_non_empty_string`, `params::require_hour`.
* **Constants**: `consts::LEGACY_USER_AGENT`, `consts::RETRY_AFTER_MAX`,
  `endpoints::dhcp::{DHCP_MODE_AUTOMATIC, DHCP_MODE_MANUAL, DHCP_MODE_CUSTOM}`, and the route
  constants for the new endpoints (`routes::networks::NETWORKS_SET_HIDE_5G`, …).
* `Transport::user_agent()` returns the effective `User-Agent`.

There is still no `close()`: dropping a `Client` releases its connection pool, and there is no
closed state to re-enter.

### Checklist

- [ ] Grep for `Error::Authentication {` and `Error::RateLimit {` — add `status` or `..`; build local auth errors with `Error::authentication(..)`.
- [ ] Grep for `.status()` on errors — `Authentication` and `RateLimit` now return `Some(..)`.
- [ ] Grep for the nine data-usage reads — add `None` (or a `RequestTimeout`) before `network_id` / `parent`.
- [ ] Grep for `set_session_token(`, `clear_session_token(`, `clear_auth_data(` — add `.await`, and build with `-D warnings` so a forgotten one is an error.
- [ ] Grep for `create_burst_reporter(` — pass `None`, or switch to `client.create_burst_reporter(nid, parent)`.
- [ ] Decide on the `User-Agent`: keep the new default, or pass `consts::LEGACY_USER_AGENT`.
- [ ] Grep for `field == "link"`, `"user_agent"` and `base_url` message matches — switch to the new field names and fixed messages.
- [ ] Grep for `update_schedule(` / `delete_schedule(` with a bare id — pass the pause's path, URL or envelope.
- [ ] Grep for `set_dhcp(Some("manual")` — it now takes effect as `"custom"`, and restarts the mesh.
- [ ] Grep for `TransportBuilder::timeout(Duration::ZERO)` / `read_timeout(Duration::ZERO)` — now unbounded.
- [ ] Grep for `networks().get_account(` — use `account().get_account()`.
- [ ] Under `StorageFailures::Warn`, check `credentials_persisted()` after a session change if persistence matters to you.
- [ ] Update log alerting that matched a transport `WARN`/`ERROR` line or expected a URL in it.

---

## 1.0.0 → 2.0.0

**What broke**: five groups of changes landed together.

1. Surface that never worked against the current API, or that the API stopped serving, is
   gone — [Removed](#removed).
2. Several writes were re-pointed to the request forms the API declares, with new signatures —
   [Changed signatures](#changed-signatures).
3. `Error` variants changed shape, and three previously-unused variants (plus two new ones) are
   now raised — [Errors](#errors).
4. The session transport, credential record and refresh flow changed —
   [Session, transport and storage](#session-transport-and-storage).
5. Fourteen domain modules were added, every resource argument became id-or-URL polymorphic,
   and every domain method whose Python counterpart takes `parent=` gained it — [Added](#added).

There is no deprecation window: a removed method is a compile error, not a runtime no-op.

### Removed

| Removed in 2.0.0 | If you called it | Do this instead |
|---|---|---|
| `Client::get_settings`, `EeroApi::settings()`, `endpoints::SettingsApi` | Replace with a network read | `client.get_network(nid, false)` — the same fields live on the network envelope |
| `Client::get_password`, `EeroApi::password()`, `endpoints::PasswordApi` | Replace with a network read | `client.get_network(nid, false)` |
| `Client::set_ipv6_dns` | It toggled `ipv6_upstream`, never DNS servers | `client.set_ipv6(enabled, nid)` for the connectivity toggle; `client.set_custom_dns_ipv6(servers, nid)` for IPv6 DNS servers |
| `Client::run_insights` | Remove — the API has no such operation | None |
| `Client::run_ouicheck` | Remove — the API has no such operation | None |
| `SecurityApi::set_thread`; the `thread` argument of `configure_security` | Remove — the settings write ignores a `thread` field | `client.set_thread_enabled(enabled, nid)` (now a JSON PUT to `networks/{id}/thread`), `update_thread(..)`, `regenerate_thread_credentials(nid)` |
| `Client::set_sqm_enabled`, `set_sqm_bandwidth`, `configure_sqm`, `set_sqm_auto` | They sent bandwidth/mode fields the API never declared | `client.set_sqm(enabled, nid)` — a single toggle, sent as the `sqm` query parameter. Settings-class: may reboot the mesh |
| `Client::get_backup_network`, `get_backup_status`, `set_backup_network`, `configure_backup_network` | Fields (`phone_number`) the API never declared | `client.get_backup_internet(nid)`, `set_backup_internet(enabled, nid)`, `get_cellular_backup_usage(nid)`, `get_cellular_backup_events(nid)` |
| `Client::get_profile_schedule`, `set_profile_schedule` | They wrote a `schedule` array onto the profile — not a profile field | Schedules are sub-resources: `get_schedules(profile_id, nid)`, `create_schedule(profile_id, name, days, start, end, enabled, nid)`, `update_schedule(schedule, &options, parent)`, `delete_schedule(schedule, parent)` |
| `Client::get_blocked_applications`, `set_blocked_applications`, `update_profile_content_filter`, `update_profile_block_list` | They wrote fields a profile does not have — silent no-ops | The DNS-policies family: `block_domain_for_profiles(domain, profiles, is_delete, override_, nid)`, `set_profile_blocked_applications(profile_id, applications, nid)`, `get_dns_policy_applications(profile_id, nid)` |
| `Client::get_burst_reporters` | Remove — the endpoint returns 404 | None; the resource is POST-only. In 2.x: `client.api().burst_reporters().create_burst_reporter(network_id, reporter_data, parent)`; from 3.0.0: `client.create_burst_reporter(nid, parent)` |
| `Client::create_burst_reporter`, `Client::request_support`, `Client::reboot_network` | Wrappers the Python facade did not have at 8.0.4 (it gained them by 8.4.1, and 3.0.0 restores them) | In 2.x: `client.api().burst_reporters().create_burst_reporter(..)`, `client.api().support().request_support(network_id, request_data, parent)`, `client.api().networks().reboot_network(network_id, parent)`. From 3.0.0: `client.create_burst_reporter(nid, parent)`, `client.request_support(request_data, nid, parent)`, `client.reboot_network(nid, parent)` |
| `Client::add_to_blacklist`, `Client::remove_from_blacklist` | Use the device-level pair (3.0.0 restores both as `(mac, nid, parent)`) | `client.block_device(device_id, nid)` / `client.unblock_device(device_id, nid)`; the domain methods `client.api().blacklist().add_to_blacklist` / `remove_from_blacklist` remain |
| `consts::MAX_ERROR_BODY_CHARS` | Error messages no longer embed a body | Read `Error::envelope()` |
| `routes::Route` (the 1.0.0 route model) | Transport-level only | `routes::Resource` / `routes::Nested`; `Transport::request(method, url, query, body)` for a raw call |

```rust
// Before (1.0.0)
client.set_sqm_enabled(true, None).await?;
client.get_settings(None).await?;
client.set_ipv6_dns(true, None).await?;

// After (2.0.0) — read first; set_sqm is a settings-class write
let net = client.get_network(None, false).await?;
if net.data()["sqm"] != serde_json::json!(true) {
    client.set_sqm(true, None).await?;
}
client.set_ipv6(true, None).await?;                                     // connectivity toggle
client.set_custom_dns_ipv6(&["2001:4860:4860::8888"], None).await?;      // IPv6 DNS servers
```

### Changed signatures

| Method | 1.0.0 | 2.0.0 |
|---|---|---|
| `block_device` | `(device_id, blocked: bool, nid)` — one method for both directions | `block_device(device_id, nid)` posts `device_id` as the form field `mac` to `POST networks/{id}/blacklist`, so pass the device's MAC; `unblock_device(device_id, nid)` is `DELETE networks/{id}/blacklist/{mac}`. Both are verified live (`block_device` since 3.0.0, which dropped its warning) |
| `set_guest_network` | `(enabled, name, password, nid)` — one JSON PUT carrying the password | `(enabled, name: Option<&str>, nid)` — form-encoded PUT to the `guestnetwork` link. **`password` is gone**: `set_guest_password(password, nid)` / `clear_guest_password(nid)`. All three verified live |
| `set_nightlight` | `(eero_id, enabled, brightness, schedule_enabled, schedule_on, schedule_off, ambient_light_enabled, nid)` | `(eero_id, enabled: Option<bool>, brightness_percentage: Option<i32>, schedule: Option<Value>, nid)` — JSON PUT to the nightlight sub-resource with only the fields the API declares; `FeatureUnavailable` on an eero without a nightlight |
| `set_nightlight_schedule` | `(eero_id, enabled, on_time, off_time, nid)` | `(eero_id, schedule: Value, nid)` — the schedule object is forwarded unchanged |
| `set_nightlight_brightness` | `(eero_id, brightness, nid)` | Same positions; the parameter is `brightness_percentage` |
| `set_led`, `set_led_brightness` | JSON PUT to the eero's own URL — **the 1.0.0 write was verified to change nothing** | Same signatures; form-encoded `led_on=` / `led_brightness=` PUT to the `led_action` link, verified live. Any logic built on the old write has never actually run |
| `get_data_usage` | `(nid, payload: Option<Value>, resource: Option<&str>)` | `(start, end, cadence, timezone: Option<&str>, nid)` — query parameters only; `cadence` is `"daily"` or `"hourly"`. Each former `resource` value is an explicit method: `get_data_usage_breakdown`, `get_devices_data_usage`, `get_device_data_usage`, `get_eeros_data_usage_summary`, `get_eero_data_usage`, `get_profile_data_usage`, `get_unprofiled_devices_data_usage`, `get_unprofiled_data_usage_summary`, `get_data_usage_report_settings`, `set_data_usage_report_settings` |
| `get_ouicheck` | `(nid)` — always 404'd | `(serial, version, nid)` — both required; take them from the eero's own envelope |
| `get_insights` | `(nid, start, end, insight_type, cadence)` | `(start, end, insight_type, cadence, nid)` — `network_id` moved last; five per-resource variants added |
| `run_diagnostics` | `(nid)` — empty POST | `(nid, device: Option<&str>, symptom: Option<&str>)` — JSON POST of whichever you supply |
| `clear_custom_dns` | `(nid)` | `(family: Option<&str>, nid)` — `None`, `"ipv4"` or `"ipv6"`; retains the stored servers |
| `create_profile` | `(name, nid)` | `(name, devices: Option<&[&str]>, paused: Option<bool>, nid)` |
| `get_devices` | `(nid, refresh_cache)` | `(nid, refresh_cache, thread: Option<bool>, proxied_node: Option<bool>)` — a filtered call bypasses the cache |
| `get_eero` | `(eero_id, nid)` | `(eero_id, nid, refresh_cache)` — accepted for parity, never cached |
| `delete_reservation` | `(reservation_id, nid)` | `(reservation_id, delete_forwards: Option<bool>, nid)` |
| `clear_profile_schedule` | `-> Result<Envelope, Error>` | `-> Result<Vec<Envelope>, Error>` — one DELETE per pause |
| `configure_security` | `(wpa3, band_steering, upnp, ipv6, thread, nid)` | `(wpa3, band_steering, upnp, ipv6, nid)` |
| `logout` (`Client`, `EeroApi`, `AuthApi`) | `-> Result<Envelope, Error>` | `-> Result<bool, Error>`: `false` with no network call when not authenticated, else `true`; a failed request is logged, never propagated |
| DNS writers (`set_custom_dns`, `set_dns_mode`, `set_dns_caching`, `clear_custom_dns`) | Sent a `custom_dns` field the API does not have — no-ops | **Now take effect, and reboot every eero.** Max 2 servers per family; provider presets (`"cloudflare"`, …) are `Error::Validation` — read `data.dns.default_test_servers`. See [Troubleshooting — DNS writes](Troubleshooting#dns-writes) |

```rust
// Before (1.0.0)
client.set_guest_network(true, Some("Guest"), Some("<password>"), None).await?;
client.block_device("<device-id>", true, None).await?;
client.block_device("<device-id>", false, None).await?;
client.set_profile_schedule("<profile-id>", &time_blocks, None).await?;
client.get_data_usage(None, None, Some("network")).await?;

// After (2.0.0)
client.set_guest_network(true, Some("Guest"), None).await?;
client.set_guest_password("<password>", None).await?;
client.block_device("<device-id>", None).await?;
client.unblock_device("<device-id>", None).await?;
client.create_schedule("<profile-id>", "Bedtime", &["monday"], "22:00", "06:00", true, None).await?;
client.get_data_usage("2026-07-01T00:00:00Z", "2026-07-21T00:00:00Z", "daily", None, None).await?;
```

### Errors

Every server-driven variant now carries `envelope: Option<Value>` and `error_code: Option<String>`,
and messages are fixed catalogue labels rather than a truncated body. Pattern matches on the
old shapes will not compile:

| Response | 1.0.0 | 2.0.0 |
|---|---|---|
| Any 401 | `Error::Authentication(String)` | `Error::Authentication { message, envelope, error_code }` |
| 429 | `Error::RateLimit { retry_after }` | `Error::RateLimit { message, retry_after, envelope, error_code }`; also raised for `error.rate.limit` on any status |
| Any 404 | `Error::Api { status: 404, .. }` | `Error::NotFound { status, message, envelope, error_code }` |
| 403 with `error.access.denied` | `Error::Api { status: 403, .. }` | `Error::AccessDenied { .. }` (new); `is_auth_error()` is `false` |
| 400 with a form-error string | `Error::Api { status: 400, .. }` | `Error::Validation { field: "request", envelope, error_code, .. }` |
| Premium / feature-unavailable strings, any status | `Error::Api` | `Error::PremiumRequired { status: Option<u16>, .. }` / `Error::FeatureUnavailable { .. }` — now actually raised |
| `error.app.version.blocked`, any status | `Error::Api` | `Error::ClientBlocked { .. }` (new) |
| Everything else | `Error::Api { status, message, url }` with the body pasted (truncated) into `message` | `Error::Api { status, message, envelope, error_code, url }`; `message` is the catalogue string or `"unrecognised error string"` |
| Local validation | `Error::Validation { field, message }` | `Error::Validation { field, message, envelope: None, error_code: None }` |

```rust
// Before (1.0.0)
Err(Error::Authentication(msg)) => eprintln!("{msg}"),
Err(Error::Api { status: 404, .. }) => ...,
Err(Error::Api { message, .. }) if message.contains("error.session.expired") => ...,

// After (2.0.0)
Err(Error::Authentication { error_code, .. }) if error_code.as_deref() == Some("error.session.expired") => ...,
Err(Error::NotFound { .. }) => ...,
Err(e) => { let group = rusteero::classify_error_code(e.error_code()); ... }
```

New: `Error::envelope()`, `Error::error_code()`, `Error::status()`, the `rusteero::errors`
module with the catalogue (`ErrorGroup`, `classify_error_code`, `message_for_error_code`,
`error_for_response`, the `*_ERRORS` slices), and `rusteero::{ErrorGroup, classify_error_code}`
at the crate root.

### Session, transport and storage

**What changes for a caller: usually nothing.** `LoginFlow`/`PendingLogin`/`Session::from_token`/
`Session::from_env`, `set_session_token`, `clear_session_token` and `is_authenticated` keep their
shapes, and a stored 1.0.0 session keeps working after its record is migrated. Underneath:

| 1.0.0 | 2.0.0 |
|---|---|
| Token sent as the `s=` cookie | Sent as the `X-User-Token` header, plus the legacy `Cookie: s=<token>` while `send_legacy_cookie(true)` (default). Both withheld from any URL off the configured host, scheme and port |
| `login` / `verify` / `logout` sent JSON | Form-encoded bodies (`login=`, `code=`, and a field named `Cookie` carrying `s=<token>`). `resend` sends `{}`; refresh sends the literal JSON string `""` |
| reqwest's default `User-Agent` | `Accept: application/json`, `User-Agent: eero/3.0 (iPhone; iOS 17.0)` and `X-Accept-Language: en-US` on every request (`user_agent`, `accept_language` builder options). 3.0.0 changes the default `User-Agent`; see [above](#behaviour-changes) |
| `Session` carried a client-fabricated 30-day expiry and an optional refresh token; `Session::expiry()`, `Session::refresh_token()` | `Session` is a token only. Both accessors are gone; `is_valid()` / `is_authenticated()` mean "a token is present". Any "days remaining" logic has nothing to read |
| `refresh_session` looked for a refresh token the API never issues, falling back to `account/refresh` | `POST /2.2/login/refresh` authenticated by the session token itself; concurrent callers share one in-flight refresh; the server-issued token is ignored. A terminal 401 from the refresh endpoint clears the stored credential unless it carries a verification or `error.session.refresh` string |
| `PendingLogin::verify` read a fresh `Set-Cookie` if the server sent one | The login token becomes the session token; the verify response body is not consulted |
| Stored record `{"session_id", "refresh_token", "session_expiry"}` (plus the legacy `user_token` read alias) | `{"session_id": "…", "schema_version": 2}`. A record without `schema_version` is migrated on first load (token kept, other fields dropped, re-saved, read back); the migration never fails the load |
| `ChainedStore` fallback branch rarely reachable | `save` genuinely falls through to the fallback when the primary fails, clearing the stale primary entry first |
| No GET retry | `get_retries(n)` — bounded retry for `GET` on transport error / `5xx`, never for writes, `4xx` or `429` |

> If something outside the library reads or writes the credential file, stop reading
> `session_expiry` and `refresh_token`; read `session_id` only, and write
> `{"session_id": ..., "schema_version": 2}`.

### Added

* **Fourteen domain modules** on `EeroApi`, each with `Client` wrappers: `account`,
  `backup_access_points`, `ddns`, `dhcp`, `dns_policies`, `entitlements`, `events`, `members`,
  `notifications`, `permissions`, `power_saving`, `subnets`, `wan`, `wpa3` — plus node/port
  actions, LED cycle, nightlight override and support reads on `eeros`, and MLO / fast
  transition / Passpoint / proxied nodes on `security`. See
  [Rust API — the 37 domain accessors](Rust-API#eeroapi-the-37-domain-accessors).
* **Id-or-URL polymorphism.** Every resource argument also accepts the resource's API path or its
  absolute API-host URL. Existing bare-id calls keep working; a URL on another host is
  `Error::Validation`.
* **`parent: Option<&Value>`** (last parameter) on every `EeroApi` domain method whose `eero-api`
  counterpart takes `parent=`. `Client` passes its cached network / eero / device envelopes
  automatically — so a network-scoped call made
  while a fresh `get_network()` result is cached goes to the link the API published, which may be
  on a different version than the method's template. (3.0.0 pins a few methods to one version
  whatever the link says; see
  [Rust API — `parent`](Rust-API#parent-use-the-link-the-api-published).)
* The link helpers at the crate root: `resolve_link`, `self_url`, `resource_url`,
  `sub_resource_url`, `join_api_path`; `rusteero::params` (`validate_cadence`,
  `resolve_network_url`, `resolve_nested_url`); `rusteero::links::warn_uncharacterised_write`.
* `ClientBuilder::accept_language`, `send_legacy_cookie`, `get_retries`.
* On `Client`: `get_device_priority`, `update_device_via_link`, `set_device_type`,
  `get_device_labels`, `set_device_labels`, `set_location`, `get_connections`, `get_speed_tests`,
  `get_guest_network`, `set_network_password`, `clear_network_password`, `apply_update`,
  `update_forward`, `update_thread`, `regenerate_thread_credentials`, `set_custom_dns_ipv4`,
  `set_custom_dns_ipv6`, the five per-resource insights reads and the data-usage family.
* `consts::API_HOST`, `api_endpoint(version)`, `API_VERSION_DEFAULT`, `API_VERSION_DEVICE_WRITES`,
  `API_VERSION_MULTISTATICIP`, `API_VERSION_SECONDARY_WAN`, `DEFAULT_USER_AGENT`,
  `DEFAULT_ACCEPT_LANGUAGE`, `GET_RETRY_DELAY`, `CREDENTIAL_SCHEMA_VERSION`,
  `SESSION_REFRESH_GUARD_TIMEOUT`, `LOGOUT_COOKIE_FIELD_NAME`, `SESSION_COOKIE_PREFIX`.
* `transport::RequestBody` (`None`, `Json`, `Form`, `EmptyJsonString`) and
  `Transport::request` / `resource` / `nested`.

### Checklist

- [ ] Grep for `get_settings(`, `get_password(`, `.settings()`, `.password()` — replace with `get_network`.
- [ ] Grep for `set_ipv6_dns(` — split into `set_ipv6` and/or `set_custom_dns_ipv6`.
- [ ] Grep for `run_insights(`, `run_ouicheck(`, `get_burst_reporters(` — remove.
- [ ] Grep for `set_thread(` and `configure_security(` with a `thread` argument — drop it; `set_thread_enabled` still exists but is now a different, unverified write.
- [ ] Grep for `set_sqm_enabled(`, `configure_sqm(`, `set_sqm_bandwidth(`, `set_sqm_auto(` — replace with `set_sqm` behind a read-compare-skip.
- [ ] Grep for `get_backup_network(`, `get_backup_status(`, `set_backup_network(`, `configure_backup_network(` — replace with the `*_backup_internet` / `cellular_backup` methods.
- [ ] Grep for `get_profile_schedule(`, `set_profile_schedule(`, `time_blocks` — move to the schedule sub-resource methods; handle `clear_profile_schedule`'s `Vec<Envelope>`.
- [ ] Grep for `update_profile_content_filter(`, `update_profile_block_list(`, `get_blocked_applications(`, `set_blocked_applications(` — move to the DNS-policies family (premium).
- [ ] Grep for `set_guest_network(` with a password — split into `set_guest_network` + `set_guest_password`.
- [ ] Grep for `block_device(` with a `bool` — split into `block_device` / `unblock_device`; `add_to_blacklist(`/`remove_from_blacklist(` on `Client` — same in 2.x (3.0.0 brings them back with a `(mac, nid, parent)` signature).
- [ ] Grep for `set_nightlight(`, `set_nightlight_schedule(` — switch to `brightness_percentage` / `schedule: Value`.
- [ ] Grep for `set_led(`, `set_led_brightness(` — audit: the 1.0.0 write did nothing, so any behaviour that assumed the LED changed has never run.
- [ ] Grep for `get_data_usage(` — pass `start`, `end`, `cadence`; move per-resource reads to the explicit methods. `get_ouicheck(` — pass `serial`, `version`. `get_insights(` — `network_id` is now last.
- [ ] Grep for `run_diagnostics(`, `clear_custom_dns(`, `create_profile(`, `get_devices(`, `get_eero(`, `delete_reservation(` — add the new arguments.
- [ ] Grep for `Error::Authentication(`, `RateLimit {`, `Api {` patterns — update to the new field sets; add `NotFound`, `AccessDenied`, `Validation` arms where you matched on `status`.
- [ ] Grep for `.contains("error.` on an error message — switch to `error_code()` / `classify_error_code`.
- [ ] Grep for `expiry()`, `refresh_token()`, `session_expiry`, `MAX_ERROR_BODY_CHARS` — remove.
- [ ] `logout()` now returns `bool` — drop any `Envelope` handling on it.
- [ ] Audit every DNS write call site: they now take effect and reboot every eero.
- [ ] Any automation issuing a settings-class write (`set_sqm`, `set_dhcp`, `set_connection_mode`, `set_nat_port_randomization`, `set_mlo_mode`, the security setters, `set_wpa3_per_band`, `set_power_saving`, `set_subnets_config`, `set_multistaticip`, secondary WAN, the DNS writes) must read, compare and skip.

---

## Upgrading safely

- **Pin a major version** (`rusteero = "3"`) or a git tag, never the default branch — this crate
  follows an undocumented, reverse-engineered API, and breaking changes ship as majors on
  purpose.
- **Read `CHANGELOG.md`** for every major between your pin and the target, and apply each
  section above in order (2.x → 3.0.0 assumes you have already done 1.0.0 → 2.0.0).
- **Let the compiler find breakage**: every removed or re-signatured method is a compile error,
  not a runtime no-op. What it cannot catch:
  - in 3.0.0, a session setter called as a bare statement without `.await` (a warning only), a
    deprecated `Some(..)` passed as `reporter_data`, and every item under
    [Behaviour changes](#behaviour-changes), the new `User-Agent` first;
  - in 2.0.0, the DNS writers and `set_led`/`set_led_brightness` (same names, now effective).

---

## 🔗 Related Pages

- [📖 Rust API](Rust-API) — the full 3.0.0 surface
- [⚙️ Configuration](Configuration) — the schema-2 credential record and its migration
- [🔧 Troubleshooting](Troubleshooting) — the DNS write path in detail
- [🏠 Home](Home)
