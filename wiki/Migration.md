# 🔀 Migration

Upgrade guide between `rusteero` major releases. The crate tracks the Python
[`eero-api`](https://github.com/fulviofreitas/eero-api) library, so each `rusteero` major maps
onto an `eero-api` major: 1.0.0 ported `eero-api` 6.2.0, 2.0.0 ports `eero-api` 8.0.4. Where a
change below has a Python counterpart, the same section of the Python
[Migration](https://github.com/fulviofreitas/eero-api/wiki/Migration) page (v6.x → v7.0.0 and
v7.x → v8.0.0) gives the wire-level reasoning.

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
   and every domain method gained `parent` — [Added](#added).

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
| `Client::get_burst_reporters` | Remove — the endpoint returns 404 | None; the resource is POST-only: `client.api().burst_reporters().create_burst_reporter(network_id, reporter_data, parent)` |
| `Client::create_burst_reporter`, `Client::request_support`, `Client::reboot_network` | Wrappers the Python facade never had | `client.api().burst_reporters().create_burst_reporter(..)`, `client.api().support().request_support(network_id, request_data, parent)`, `client.api().networks().reboot_network(network_id, parent)` |
| `Client::add_to_blacklist`, `Client::remove_from_blacklist` | Use the device-level pair | `client.block_device(device_id, nid)` / `client.unblock_device(device_id, nid)`; the domain methods `client.api().blacklist().add_to_blacklist` / `remove_from_blacklist` remain |
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
| `block_device` | `(device_id, blocked: bool, nid)` — one method for both directions | `block_device(device_id, nid)` sends the device's MAC to `POST networks/{id}/blacklist`; `unblock_device(device_id, nid)` is `DELETE networks/{id}/blacklist/{mac}` (verified) |
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
| reqwest's default `User-Agent` | `Accept: application/json`, `User-Agent: eero/3.0 (iPhone; iOS 17.0)` and `X-Accept-Language: en-US` on every request (`user_agent`, `accept_language` builder options) |
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
* **`parent: Option<&Value>`** on every `EeroApi` domain method (last parameter). `Client` passes
  its cached network / eero / device envelopes automatically — so a network-scoped call made
  while a fresh `get_network()` result is cached goes to the link the API published, which may be
  on a different version than before (`routing` on `2.3`, for example).
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
- [ ] Grep for `block_device(` with a `bool` — split into `block_device` / `unblock_device`; `add_to_blacklist(`/`remove_from_blacklist(` on `Client` — same.
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

- **Pin the git revision** (`rev = "…"` or `tag = "…"`) rather than tracking the default branch —
  this crate follows an undocumented, reverse-engineered API, and breaking changes ship as
  majors on purpose.
- **Read `CHANGELOG.md`** for every major between your pin and the target.
- **Let the compiler find breakage**: every removed or re-signatured method in this release is a
  compile error, not a runtime no-op. The two things it cannot catch are the DNS writers (same
  names, now effective) and `set_led`/`set_led_brightness` (same names, now effective).

---

## 🔗 Related Pages

- [📖 Rust API](Rust-API) — the full 2.0.0 surface
- [⚙️ Configuration](Configuration) — the schema-2 credential record and its migration
- [🔧 Troubleshooting](Troubleshooting) — the DNS write path in detail
- [🏠 Home](Home)
