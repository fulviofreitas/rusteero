# 🦀 Rust API

Full reference for `rusteero`'s public surface, ported from the Python
[`eero-api`](https://github.com/fulviofreitas/eero-api) library at **v8.0.4**. Method names are
the Python names verbatim, in `snake_case` (including the `get_` prefix) — deliberate, so a
method can be searched for across both libraries by name. Every endpoint method returns
`Result<Envelope, rusteero::Error>` over the raw `{"meta": …, "data": …}` wire payload; nothing
is transformed on the way through.

See [PARITY.md](https://github.com/fulviofreitas/rusteero/blob/master/PARITY.md) in the
repository for the exhaustive method-by-method table, and the crate's own rustdoc for every
signature and its `# Errors` contract.

## Quick Start

```rust
use rusteero::{Client, Session};

#[tokio::main]
async fn main() -> Result<(), rusteero::Error> {
    let client = Client::builder()
        .session(Some(Session::from_env("RUSTEERO_SESSION_TOKEN")?))
        .build()
        .await?;

    let account = client.get_account(false).await?;
    println!("{}", account.into_value());   // exact wire JSON
    Ok(())
}
```

`ClientBuilder::build()` is **async** — it may need to load a session from a configured
[`CredentialStore`](Configuration) — so every example on this page `.await`s it.

---

## The three layers

```
Client            cache, network-id resolution, cached parent envelopes     (eero-api: EeroClient)
  └─ EeroApi      one accessor per domain module (37), one shared Transport (eero-api: EeroAPI)
       └─ Transport   headers, credential placement, status → Error, GET retry, refresh-and-replay
```

| Layer | Reach it via | When to use it |
|---|---|---|
| `Client` | `Client::builder()` | The default. Optional `network_id`, cached reads, cache invalidation on writes, cached envelopes passed as `parent` for you |
| `EeroApi` | `client.api()` or `EeroApi::new(transport)` | You already know the network id and want no cache. `network_id: &str` is required and comes first; every method takes `parent: Option<&Value>` |
| `Transport` | `client.api().transport()` or `Transport::builder()` | A raw call to a URL you resolved yourself: `Transport::request(method, url, query, body)` with a `RequestBody` |

```rust
use rusteero::transport::RequestBody;
use rusteero::{resolve_link, EeroApi};

// EeroApi: no cache, explicit network id, explicit parent.
let net = client.api().networks().get_network("<network-id>", None).await?;
let eeros = client.api().eeros().get_eeros("<network-id>", Some(net.as_value())).await?;

// Transport: follow a link the API published, by hand.
let host = client.api().transport().api_host();
if let Some(url) = resolve_link(host, net.as_value(), "eeros")? {
    let raw = client
        .api()
        .transport()
        .request(reqwest::Method::GET, url, &[], RequestBody::None)
        .await?;
}
```

`RequestBody` is `None`, `Json(Value)`, `Form(Vec<(String, String)>)` or `EmptyJsonString`
(the literal two-byte body `""` several `POST` actions require).

---

## Client construction

```rust
use rusteero::storage::MemoryStore;
use rusteero::transport::StorageFailures;
use rusteero::{Client, Session};
use std::sync::Arc;
use std::time::Duration;

let client = Client::builder()
    .session(Some(session))                          // Option<Session>; omit to load from the store
    .store(Some(Arc::new(MemoryStore::new())))        // Option<Arc<dyn CredentialStore>>; None = no persistence
    .storage_failures(StorageFailures::Warn)          // Warn (default) | Fatal
    .cache_ttl(Duration::from_secs(60))               // default 60 s; Duration::ZERO disables cache reads
    .user_agent(Some("my-tool/1.0".to_owned()))       // default: consts::DEFAULT_USER_AGENT
    .accept_language("en-US")                         // X-Accept-Language; default consts::DEFAULT_ACCEPT_LANGUAGE
    .send_legacy_cookie(true)                         // also send Cookie: s=<token>; default true
    .get_retries(0)                                   // extra GET attempts on transport error / 5xx; default 0
    .base_url("http://127.0.0.1:8080")                // tests only: derives /2.2 and /2.3 from one root
    .http(reqwest::Client::new())                     // optional custom HTTP client (see warning)
    .build()
    .await?;
```

| Option | Type | Default | Notes |
|---|---|---|---|
| `session` | `Option<Session>` | `None` | An explicit session wins over whatever the store holds; the store is still installed for later persistence |
| `store` | `Option<Arc<dyn CredentialStore>>` | `None` | Loaded once at `build()` when no explicit session was given |
| `storage_failures` | `StorageFailures` | `Warn` | `Fatal` turns any store failure (including the initial load) into `Error::Storage` |
| `cache_ttl` | `Duration` | 60 s | TTL of the eight cached getters; `Duration::ZERO` disables reads but not writes |
| `user_agent` | `Option<String>` | `eero/3.0 (iPhone; iOS 17.0)` | Printable ASCII only |
| `accept_language` | `impl Into<String>` | `en-US` | Printable ASCII only, validated at `build()` |
| `send_legacy_cookie` | `bool` | `true` | `false` sends only the `X-User-Token` header |
| `get_retries` | `u32` | `0` | Never applies to writes, never to `4xx`/`429`; distinct from the one-shot 401 refresh-and-replay |
| `base_url` | `impl Into<String>` | real Eero hosts | `Error::Validation` at `build()` if not an absolute URL |
| `http` | `reqwest::Client` | built by the crate | **Discards redirect refusal and the request/read timeouts** — rebuild both yourself |

Every setter takes `self` by value and returns `Self`; `build()` is the only fallible and only
`async` step.

---

## Authentication

### Interactive login flow (separable from the client)

The one-time-code handshake is a type-state pair, `LoginFlow` → `PendingLogin` → `Session`, and
needs neither a `Client` nor a `CredentialStore`:

```rust
use rusteero::auth::flow::LoginFlow;

let pending = LoginFlow::new(None)?.start("you@example.com").await?;  // email or phone; form-encoded
pending.resend().await?;                                              // optional: ask for a new code
let session = pending.verify("123456").await?;                        // consumes `pending` → Session
```

`LoginFlow::new(None)` builds a transport against the real Eero cloud hosts; pass
`Some(reqwest::Client)` to supply your own, or use `LoginFlow::with_transport(transport)` to
point it at a test server. There is no `Client::login`/`Client::verify` — an unverified login
token can never reach an authenticated endpoint by construction, since only
`PendingLogin::verify` can produce a `Session`. The login token itself becomes the session
token; the verify response body is not consulted.

### Headless / CI: inject a token directly

```rust
use rusteero::Session;

let session = Session::from_token("<token>");                       // no validation, no expiry
let session = Session::from_env("RUSTEERO_SESSION_TOKEN")?;         // validated: non-empty, printable ASCII
let client = Client::builder().session(Some(session)).build().await?;
```

A `Session` is nothing more than a token: **there is no client-side expiry**. `is_valid()` is a
token-presence check and the server is the sole authority on whether the token still works,
signalled by a 401.

### Check / refresh / logout

```rust
client.is_authenticated();                             // local: a non-empty token is configured
client.session();                                      // Option<Session>; the token stays a SecretString
client.logout().await?;                                // -> bool; POST logout, then clears session, store and cache
client.set_session_token("<token>")?;                  // validate + install + persist; clears the cache
client.clear_session_token()?;                         // clear in memory and in the store; clears the cache
client.api().auth().refresh_session().await?;          // -> bool; POST login/refresh with the current token
client.api().auth().ensure_authenticated().await?;     // Err(Error::Authentication) when no token
client.api().auth().clear_auth_data()?;                // same as clear_session_token at v8.0.4
```

`logout()` returns `Ok(false)` **with no network call** when no session is configured, and
otherwise `Ok(true)` regardless of the network outcome (a failed `POST logout` is logged at
`WARN`); the session is destroyed locally and in the store either way. Under
`StorageFailures::Fatal` a failed store clear surfaces as `Error::Storage`.

You never need to call `refresh_session()` yourself. When a request answers `401` with
`meta.error == "error.session.refresh"`, the transport refreshes once (concurrent callers share
a single in-flight refresh) and replays the request once. A terminal 401 — `error.session.expired`,
`.invalid`, `.revoked`, or any string outside the verification/refresh groups — from the refresh
endpoint clears the stored credential; see [Troubleshooting](Troubleshooting#401-errorsessionexpired-vs-errorsessionrefresh).

---

## Network targeting: `network_id: Option<&str>`

Most `Client` methods take `network_id: Option<&str>` and resolve it in this order:

1. The explicit `network_id`, if non-empty (an empty string counts as `None`).
2. `client.preferred_network_id()`, set via `client.set_preferred_network("<id>")` — in-memory
   only, lost on restart, never synced with the server — or populated as a side effect of the
   first `get_networks()` call when it was unset.
3. **Only for the 24 methods below**, auto-discovery: the first entry of `get_networks(false)`.
4. Otherwise `Error::MissingNetworkId`.

Methods that auto-discover: `get_network`, `get_eeros`, `get_eero`, `reboot_eero`,
`set_guest_network`, `run_speed_test`, `get_devices`, `get_device`, `set_device_nickname`,
`block_device`, `unblock_device`, `pause_device`, `update_device_via_link`, `set_device_type`,
`get_device_labels`, `set_device_labels`, `get_profiles`, `get_profile`, `get_profile_devices`,
`create_profile`, `rename_profile`, `delete_profile`, `pause_profile`, `set_profile_devices`.

Everything else — every settings-class read or write, `get_device_priority`, the data-usage,
insights, DNS, security, SQM, thread, members, notifications, DHCP, WPA3, power-saving, subnets,
WAN, DDNS and backup families — goes straight to `Error::MissingNetworkId` when neither step 1
nor 2 yields an id. The cheap fix: call `get_networks(false)` once, early.

**Parameter position.** `network_id` is the *last* parameter of almost every wrapper. The
exceptions, where it comes first because the method has no leading resource argument, are the
cached list getters (`get_network`, `get_eeros`, `get_devices`, `get_profiles`),
`get_speed_tests`, `run_diagnostics`, `get_transfer_stats`, `set_power_saving`,
`create_power_saving_schedule` and (after `schedule_id`) `update_power_saving_schedule`.

```rust
client.set_preferred_network("<network-id>");
let net     = client.get_network(None, false).await?;              // network_id first here
let devices = client.get_devices(None, false, None, None).await?;  // (network_id, refresh_cache, thread, proxied_node)
let device  = client.get_device("<device-id>", None, false).await?; // resource first, network_id second
client.set_upnp(true, Some("<network-id>")).await?;                // value first, network_id last
```

`EeroApi` has no preferred-network concept: `network_id: &str` is required and comes first on
every domain method.

---

## Ids, paths, URLs and `parent`

### Every resource argument accepts three forms

Wherever a method takes a `network_id`, `eero_id`, `device_id`/`mac`, `profile_id`, `forward`,
`reservation`, `invite_id` or `schedule`, you may pass:

| Form | Example | What the crate does |
|---|---|---|
| Bare id | `"<network-id>"` | Substituted into the method's template on that family's API version. Must match `[A-Za-z0-9][A-Za-z0-9._:-]*` with no `..` |
| API path | `"/2.2/networks/<network-id>"` | Joined onto the configured API host, keeping whatever version prefix the path carries |
| Absolute URL | `"https://api-user.e2ro.com/2.2/networks/<network-id>"` | Used as-is after validation |

An absolute URL is accepted only when its scheme and hostname match the configured API host
(`Transport::api_host()`); another host, `http://` against an `https://` host, a userinfo trick
(`https://api-user.e2ro.com@evil.example/…`) or a suffix trick
(`https://api-user.e2ro.com.evil.example/…`) is `Error::Validation` before any request is sent.
The session credential is additionally withheld from any request whose URL is not on the
configured host, scheme and port.

A *child* argument given as a path or URL (a profile, a MAC, an invite, a reservation or forward
on `delete_*`) must also belong to the addressed network:
`/<version>/networks/<that network>/<family>/<id>` with a single-segment id and no query or
fragment, else `Error::Validation { field: "child", .. }`.

### `parent`: use the link the API published

Every domain method on `EeroApi` takes `parent: Option<&Value>` — the envelope of the resource
you already hold (a full `{"meta", "data"}` response or just its `data` object). When supplied,
the URL is resolved from that envelope's published link (`resources.settings`,
`resources.led_action`, `resources.schedules`, the resource's own `url`, …) instead of a template,
so the request follows the version the API actually serves the resource on (`routing`, for
instance, is published on `2.3` while the template default is `2.2`). The envelope is only ever
read; the raw-response contract is untouched.

```rust
let net = client.api().networks().get_network("<network-id>", None).await?;

// Uses net.data.resources.settings rather than the template.
client.api().sqm().set_sqm("<network-id>", true, Some(net.as_value())).await?;

// The guest-network password write wants the *guest network's* envelope.
let guest = client.api().networks().get_guest_network("<network-id>", Some(net.as_value())).await?;
client.api().networks().set_guest_password("<network-id>", "<new-password>", Some(guest.as_value())).await?;
```

**`Client` does this for you.** When a fresh cached envelope exists — the network from
`get_network()`, the eero from a cached `get_eeros()` list, the device from a cached
`get_device()` — the wrapper passes it as `parent` automatically. `update_schedule` and
`delete_schedule` take `parent` explicitly (the pause's own envelope) since they have no
`network_id` at all.

### The link helpers

Exported from the crate root; none performs I/O or mutates an envelope. Each takes the API host
as an explicit `&Url` (`client.api().transport().api_host()`).

| Helper | Returns |
|---|---|
| `resolve_link(host, parent, name)` | `Ok(Some(Url))` for `parent["resources"][name]` (or `parent["data"]["resources"][name]`), `Ok(None)` when absent |
| `self_url(host, parent)` | The envelope's own `url`, or `Ok(None)` |
| `resource_url(host, id_or_url, template, version)` | Absolute URL from a bare id, a path or an absolute URL; `template` must hold exactly one `{id}` |
| `sub_resource_url(host, id_or_url, template, link, parent, version)` | `resolve_link` when it yields a URL, otherwise `resource_url` |
| `join_api_path(host, path)` | Host + a host-relative path, version prefix preserved |
| `rusteero::util::id_from_url(id_or_url)` | Trailing path segment of a URL, or the bare id unchanged |

`version` is `rusteero::routes::ApiVersion::V2_2` or `V2_3`. The per-family versions live in
`rusteero::consts` (`API_VERSION_DEFAULT`, `API_VERSION_DEVICE_WRITES`,
`API_VERSION_MULTISTATICIP`, `API_VERSION_SECONDARY_WAN`).

---

## Caching

Exactly eight `Client` methods read the cache: `get_account`, `get_networks`, `get_network`,
`get_eeros`, `get_devices`, `get_device`, `get_profiles`, `get_profile`. Each takes a
`refresh_cache: bool`: `true` skips the cache read but still writes the fresh response back.
`get_eero` accepts `refresh_cache` for signature parity but is always a live call, and a
`get_devices` call with a `thread` or `proxied_node` filter never reads or writes the cache.

Rules inherited from `eero-api` and reproduced deliberately:

* A cached envelope whose value is JSON `null`, `false`, `0`, `""`, `[]` or `{}` is treated as a
  miss and refetched.
* `get_networks()` falls back to `GET /account` and synthesises `data.networks` from it when
  `/networks` comes back empty; a failure of that fallback is returned as `Err`, not swallowed.
* Auto-discovery always calls `get_networks(false)`, so a cached "zero networks" answer can
  keep discovery failing for one TTL window.
* `clear_cache()` drops every entry; `logout()`, `set_session_token()` and
  `clear_session_token()` call it for you.

Writes invalidate the entries they affect and nothing else:

| Write family | Invalidated on success |
|---|---|
| `set_account_name`, `verify_account_email`, `verify_account_phone`, `set_account_consents` | `account` |
| Network writes (`set_network_name`, `set_network_password`, `clear_network_password`, guest network, `run_speed_test`), DNS, security, SQM, thread, DHCP (`set_dhcp`, `set_connection_mode`, `set_nat_port_randomization`), DDNS, WPA3, WAN config, `set_subnets_config`, `delete_subnet`, `set_backup_internet`, `set_notification_settings`, `set_power_saving`, `apply_update`, `set_data_usage_report_settings`, the network-level DNS-policy writes | `network[<nid>]` |
| `reboot_eero`, `set_location`, `set_led`, `set_led_brightness`, `set_nightlight`, `node_action`, `port_action`, `nightlight_override` | `eeros[<nid>]` |
| Device writes (`set_device_nickname`, `pause_device`, `block_device`, `unblock_device`, `set_device_type`, `set_device_labels`, `set_device_secondary_wan_access`) | `devices[<nid>,<did>]` and `devices[<nid>]` |
| `update_device_via_link` | as above, plus every `profiles` entry of the network |
| `create_profile`, the per-profile DNS-policy writes | `profiles[<nid>]` |
| `rename_profile`, `delete_profile`, `pause_profile`, `set_profile_devices`, `set_profile_blocked_applications` | `profiles[<nid>,<pid>]` and `profiles[<nid>]` |
| Schedules (profile and power-saving), reservations, forwards, members, backup access points, `set_subnet_content_filters`, `set_pppoe`, `set_nightlight_brightness`, `set_nightlight_schedule`, `led_cycle`, `mark_notifications_read`, `set_push_settings`, `set_account_email`, `set_account_phone` | nothing |

`account` and `networks` are never invalidated by any network-scoped write; pass
`refresh_cache = true` when you need them fresh.

---

## The `Client` surface, by domain

Signatures below are the `Client` wrappers. Every one returns `Result<Envelope, Error>` unless
noted; `nid` abbreviates `network_id: Option<&str>`.

### Account and session-wide

```rust
client.get_account(refresh_cache)
client.get_networks(refresh_cache)
client.set_account_name(name)            client.set_account_email(email)      client.verify_account_email(code)
client.set_account_phone(phone)          client.verify_account_phone(code)    client.set_account_consents(marketing_emails)
client.get_sms_countries()               client.get_premium_customer()        client.set_push_settings(&[("networkOffline", true)])
client.query_invite(invite_code)
```

### Networks and guest network

```rust
client.get_network(nid, refresh_cache)
client.get_premium_status(nid)
client.get_guest_network(nid)
client.set_guest_network(enabled, name: Option<&str>, nid)      // verified live; no password here
client.set_guest_password(password, nid)                        // verified live
client.clear_guest_password(nid)                                // verified live
client.run_speed_test(nid)                                      // verified live; result lands in get_speed_tests ~1 min later
client.get_speed_tests(nid, limit: Option<u32>, start_time: Option<&str>, end_time: Option<&str>)
client.set_network_name(name, nid)                              // form-encoded; disconnects clients
client.set_network_password(password, nid)                      // disconnects clients
client.clear_network_password(nid)
```

There is no `Client::reboot_network`; it lives only on the domain API:
`client.api().networks().reboot_network(network_id, parent)`.

### Eeros (mesh nodes)

```rust
client.get_eeros(nid, refresh_cache)
client.get_eero(eero_id, nid, refresh_cache)                    // never cached
client.reboot_eero(eero_id, nid)                                // verified live: only the targeted node reboots
client.set_location(eero_id, location, nid)
client.get_connections(eero_id, nid)
client.get_led_status(eero_id, nid)
client.set_led(eero_id, enabled, nid)                           // verified live
client.set_led_brightness(eero_id, brightness: i32, nid)        // verified live; 0–100
client.get_nightlight(eero_id, nid)
client.set_nightlight(eero_id, enabled: Option<bool>, brightness_percentage: Option<i32>, schedule: Option<Value>, nid)
client.set_nightlight_brightness(eero_id, brightness_percentage: i32, nid)
client.set_nightlight_schedule(eero_id, schedule: Value, nid)
client.node_action(eero_id, action, nid)                        // "POWER_CYCLE_ALL_PORTS" | "POWER_CYCLE_ALL_PORTS_AND_REBOOT"
client.port_action(eero_id, interface_number: u32, action, nid)
client.led_cycle(eero_serial, colors: &[String], duration: u32, time_per_color: u32)
client.nightlight_override(eero_id, brightness_percentage: i32, nid)
client.get_eero_support(eero_serial)
```

### Devices (connected clients)

```rust
client.get_devices(nid, refresh_cache, thread: Option<bool>, proxied_node: Option<bool>)
client.get_device(device_id, nid, refresh_cache)
client.get_device_priority(device_id, nid)                      // uncached read; never auto-discovers
client.set_device_nickname(device_id, nickname, nid)            // verified; JSON PUT on API 2.3
client.pause_device(device_id, paused, nid)                     // verified; JSON PUT on API 2.3
client.block_device(device_id, nid)                             // GET the device for its MAC, then POST /blacklist
client.unblock_device(device_id, nid)                           // verified; DELETE /blacklist/{mac}
client.update_device_via_link(device_id, nickname: Option<&str>, paused: Option<bool>, profile: Option<&str>, nid)
client.set_device_type(device_id, device_type, nid)             // verified live
client.get_device_labels(device_id, nid)
client.set_device_labels(device_id, make_label, model_label, version_label, type_label, nid)   // verified NO-OP upstream
client.get_blacklist(nid)
```

### Profiles and schedules

```rust
client.get_profiles(nid, refresh_cache)
client.get_profile(profile_id, nid, refresh_cache)
client.get_profile_devices(profile_id, nid)
client.create_profile(name, devices: Option<&[&str]>, paused: Option<bool>, nid)
client.rename_profile(profile_id, name, nid)
client.delete_profile(profile_id, nid)
client.pause_profile(profile_id, paused, nid)
client.set_profile_devices(profile_id, device_urls: &[&str], nid)

client.get_schedules(profile_id, nid)
client.create_schedule(profile_id, name, days: &[&str], start, end, enabled, nid)
client.update_schedule(schedule, &UpdateScheduleOptions { .. }, parent: Option<&Value>)
client.delete_schedule(schedule, parent: Option<&Value>)
client.clear_profile_schedule(profile_id, nid)                  // -> Vec<Envelope>: one DELETE per pause
client.enable_bedtime(profile_id, start_time, end_time, days: Option<&[&str]>, nid)
client.set_weekday_bedtime(profile_id, start_time, end_time, nid)
client.set_weekend_bedtime(profile_id, start_time, end_time, nid)
```

Schedules are sub-resources of a profile. `update_schedule`/`delete_schedule` take the pause's
own id, path or URL and no `network_id`; `enable_bedtime`/`set_*_bedtime` each create one pause.

### DNS

```rust
client.get_dns_settings(nid)                                    // the whole network object; read data.dns / data.ipv6
client.set_dns_caching(enabled, nid)
client.set_custom_dns(dns_servers: &[&str], nid)                // split by family; max 2 per family
client.set_custom_dns_ipv4(dns_servers, nid)
client.set_custom_dns_ipv6(dns_servers, nid)
client.clear_custom_dns(family: Option<&str>, nid)              // None | "ipv4" | "ipv6"
client.set_dns_mode(mode, custom_servers: Option<&[&str]>, nid) // "auto" | "automatic" | "custom"
```

Every DNS write **reboots every eero**. Provider presets (`"cloudflare"`, `"google"`, …) are not
offered: read the API's own catalogue at `data.dns.default_test_servers`.

### Security, WPA3, SQM, thread

```rust
client.get_security_settings(nid)
client.set_wpa3(enabled, nid)          client.set_band_steering(enabled, nid)
client.set_upnp(enabled, nid)          client.set_ipv6(enabled, nid)
client.configure_security(wpa3: Option<bool>, band_steering: Option<bool>, upnp: Option<bool>, ipv6: Option<bool>, nid)
client.set_mlo_mode(mode, nid)                                  // "disabled" | "multi" | "single"
client.get_fast_transition(nid)        client.set_fast_transition(enabled, nid)
client.set_passpoint_enabled(enabled, nid)
client.set_proxied_nodes(enabled, nid)
client.get_wpa3_per_band(nid)
client.set_wpa3_per_band(band_2_4_ghz: Option<&str>, band_5_ghz: Option<&str>, nid)   // "WPA2" | "WPA2_WPA3" | "WPA3"
client.get_sqm_settings(nid)           client.set_sqm(enabled, nid)
client.get_thread(nid)                 client.set_thread_enabled(enabled, nid)
client.update_thread(thread_enable: Option<bool>, enable_credential_syncing: Option<bool>, nid)
client.regenerate_thread_credentials(nid)
```

### DHCP, WAN, DDNS, subnets, backup

```rust
client.set_dhcp(mode: Option<&str>, custom: Option<&Map<String, Value>>, custom_v2: Option<&Map<String, Value>>, nid)  // "automatic" | "manual"
client.set_connection_mode(mode, nid)                           // "BRIDGE" | "NAT"
client.set_nat_port_randomization(enabled, nid)
client.set_pppoe(eero_serial_or_id, username, password)

client.get_multistaticip(nid)                                   // API 2.3; NotFound without the feature
client.set_multistaticip(config: Value, nid)
client.set_secondary_wan_config(config: Value, nid)
client.set_device_secondary_wan_access(mac, deny, nid)

client.enable_ddns(nid)                client.disable_ddns(nid)

client.get_subnets_config(nid)         client.set_subnets_config(config: Value, nid)
client.delete_subnet(subnet_type, nid)
client.get_subnet_content_filters(subnet_id, nid)
client.set_subnet_content_filters(filters: Value, nid)

client.get_backup_internet(nid)        client.set_backup_internet(enabled, nid)
client.get_cellular_backup_usage(nid)  client.get_cellular_backup_events(nid)
client.list_backup_access_points(nid)
client.add_backup_access_point(ssid, password, uuid: Option<&str>, nid)
client.update_backup_access_point(backup_network_id, &UpdateBackupAccessPointOptions { .. }, nid)
client.delete_backup_access_point(backup_network_id, nid)
client.rearrange_backup_access_points(order: &[&str], nid)
client.discover_backup_ssids(nid)      client.start_backup_ssid_discovery(nid)
client.backup_connectivity_check(nid)
```

### Content filtering (DNS policies, premium)

```rust
client.get_advanced_content_filter(nid)
client.allow_domain(domain, add_cname: Option<bool>, reason_to_allow: Option<i64>, is_delete: Option<bool>, keep_profiles: Option<&[&str]>, nid)
client.allow_cnames(domains: &[&str], nid)
client.block_domain(domain, is_delete: Option<bool>, keep_profiles: Option<&[&str]>, nid)
client.allow_domain_for_profiles(domain, profiles: &[&str], override_: Option<bool>, add_cname: Option<bool>, reason_to_allow: Option<i64>, is_delete: Option<bool>, nid)
client.allow_cnames_for_profiles(domains, profiles, nid)
client.block_domain_for_profiles(domain, profiles, is_delete: Option<bool>, override_: Option<bool>, nid)
client.get_dns_policy_applications(profile_id, nid)
client.set_profile_blocked_applications(profile_id, applications: &[&str], nid)
```

### Data usage and insights

```rust
client.get_data_usage(start, end, cadence, timezone: Option<&str>, nid)          // cadence: "daily" | "hourly"
client.get_data_usage_breakdown(start, end, cadence: Option<&str>, timezone, nid)
client.get_devices_data_usage(start, end, cadence: Option<&str>, timezone, profile_id: Option<&str>, nid)
client.get_device_data_usage(device_mac, start, end, cadence, timezone, nid)
client.get_eeros_data_usage_summary(start, end, cadence, timezone, nid)
client.get_eero_data_usage(eero_id, start, end, cadence, timezone, nid)
client.get_profile_data_usage(profile_id, start, end, cadence, timezone, nid)
client.get_unprofiled_devices_data_usage(start, end, cadence: Option<&str>, timezone, nid)
client.get_unprofiled_data_usage_summary(start, end, cadence, timezone, nid)
client.get_data_usage_report_settings(nid)
client.set_data_usage_report_settings(cadence, notification_day, nid)

client.get_insights(start, end, insight_type, cadence, nid)
client.get_devices_insights(start, end, cadence, insight_type, nid)
client.get_device_insights(device_id, start, end, cadence, insight_type, nid)
client.get_profiles_insights(start, end, cadence, insight_type, nid)
client.get_profile_insights(profile_id, start, end, cadence, insight_type, nid)
client.get_profile_devices_insights(profile_id, start, end, cadence, insight_type, nid)
```

Note the argument order differs between the two families (`insight_type, cadence` on
`get_insights`; `cadence, insight_type` on the others), mirroring the Python signatures.

### Members, notifications, events, entitlements, permissions

```rust
client.get_members(nid)                client.get_invites(nid)
client.create_invite(role, nid)                                 // "admin" | "owner"
client.update_invite(invite_id, invite_nickname, nid)
client.delete_invite(invite_id, nid)
client.respond_to_invite(accept, invite_id: Option<&str>, invite_code: Option<&str>, nid)
client.cancel_pending_admin(nid)       client.promote_member(member_id, nid)
client.remove_admin(user_id, nid)

client.get_notification_settings(nid)
client.set_notification_settings(settings: &[(&str, bool)], nid)
client.has_unread_notifications(nid)   client.mark_notifications_read(nid)
client.get_notification_history(timestamp: Option<&str>, nid)

client.get_app_events(page_size: Option<u32>, timestamp: Option<&str>, nid)
client.get_network_scan(nid)
client.get_channel_utilization(start, end, &GetChannelUtilizationOptions { .. }, nid)

client.get_entitlement_features(nid)   client.get_upsell_features(nid)
client.get_model_capabilities(nid)     client.get_permissions(nid)
```

### Reservations, forwards, diagnostics, misc reads

```rust
client.get_reservations(nid)
client.create_reservation(reservation_data: Value, nid)
client.update_reservation(reservation_id, reservation_data: Value, nid)
client.delete_reservation(reservation_id, delete_forwards: Option<bool>, nid)
client.get_forwards(nid)
client.create_forward(forward_data: Value, nid)
client.update_forward(forward_id, forward_data: Value, nid)
client.delete_forward(forward_id, nid)

client.get_diagnostics(nid)
client.run_diagnostics(nid, device: Option<&str>, symptom: Option<&str>)
client.get_routing(nid)                client.get_support(nid)
client.get_updates(nid)                client.apply_update(nid)              // reboots every node
client.get_transfer_stats(nid, device_id: Option<&str>)
client.get_ac_compat(nid)
client.get_ouicheck(serial, version, nid)                       // both required; 404 without them
client.set_power_saving(nid, enable: Option<bool>, power_saving_schedule_enabled: Option<bool>)
client.get_power_saving_schedules(nid)
client.create_power_saving_schedule(nid, name, days: Value, start_time, end_time, enabled)
client.update_power_saving_schedule(schedule_id, nid, &UpdatePowerSavingScheduleOptions { .. })
client.delete_power_saving_schedule(schedule_id, nid)
```

`request_support` and `create_burst_reporter` have no `Client` wrapper (Python has none either);
use `client.api().support().request_support(..)` and
`client.api().burst_reporters().create_burst_reporter(network_id, reporter_data, parent)`.

### The `Options` structs

Four writes with many optional fields take an options struct instead of a long argument list.
All derive `Default` and `Clone`, and a field left `None` is omitted from the request body:

| Struct | Path | Fields |
|---|---|---|
| `UpdateScheduleOptions<'a>` | `rusteero::endpoints::schedule` | `name`, `days: Option<&[&str]>`, `start`, `end`, `enabled` |
| `UpdatePowerSavingScheduleOptions<'a>` | `rusteero::endpoints::power_saving` | `name`, `days: Option<Value>`, `start_time`, `end_time`, `enabled` |
| `UpdateBackupAccessPointOptions<'a>` | `rusteero::endpoints::backup_access_points` | `ssid`, `password`, `enabled`, `uuid`, `connectivity: Option<Value>`, `created`, `last_updated_at` |
| `GetChannelUtilizationOptions<'a>` | `rusteero::endpoints::events` | `busy_threshold: Option<u32>`, `eero_id`, `band` (one of `CHANNEL_UTILIZATION_BANDS`), `granularity: Option<u32>`, `gap_data_placeholder` |

```rust
use rusteero::endpoints::schedule::UpdateScheduleOptions;

client
    .update_schedule(
        "<schedule-id-or-url>",
        &UpdateScheduleOptions { enabled: Some(false), ..Default::default() },
        None,
    )
    .await?;
```

---

## `EeroApi`: the 37 domain accessors

`client.api()` (or `EeroApi::new(transport)`) exposes one accessor per Python domain module.
Every method takes `network_id: &str` first (where the resource is network-scoped) and
`parent: Option<&Value>` last; nothing is cached. A representative method is shown for each.

| Accessor | Domain | Representative method |
|---|---|---|
| `account()` | Account profile writes and SMS country list | `set_name(name)`, `get_sms_countries()` |
| `ac_compat()` | Legacy AC-compatibility read | `get_ac_compat(network_id, parent)` |
| `backup()` | Backup internet toggle and cellular usage/events | `get_backup_internet(network_id, parent)` |
| `backup_access_points()` | Backup Wi-Fi access points | `list(network_id, parent)`, `add(network_id, ssid, password, uuid)` |
| `blacklist()` | Blocked-device list | `get_blacklist(network_id, parent)`, `add_to_blacklist`, `remove_from_blacklist` |
| `burst_reporters()` | Burst-reporter creation (POST-only) | `create_burst_reporter(network_id, reporter_data, parent)` |
| `data_usage()` | Data-usage time series | `get_data_usage(network_id, start, end, cadence, timezone, ..)`, `get_breakdown`, `get_report_settings` |
| `ddns()` | Dynamic DNS | `enable(network_id, parent)`, `disable(network_id, parent)` |
| `devices()` | Connected clients | `get_devices(network_id, thread, proxied_node, parent)`, `block_device(network_id, mac)` |
| `dhcp()` | DHCP, connection mode, NAT randomisation, PPPoE | `set_dhcp(network_id, mode, custom, custom_v2, parent)` |
| `diagnostics()` | Diagnostics | `get_diagnostics(network_id, parent)`, `run_diagnostics` |
| `dns()` | DNS servers, mode and caching | `get_dns_settings(network_id)`, `set_dns_mode(network_id, mode, custom_servers, parent)` |
| `dns_policies()` | Content filtering (premium) | `get_advanced_content_filter`, `get_profile_applications`, `set_profile_blocked_applications` |
| `eeros()` | Mesh nodes | `get_eero(eero_id, parent)`, `reboot_eero(eero_id, parent)`, `set_led(eero_id, enabled, parent)` |
| `entitlements()` | Feature entitlements and upsells | `get_features(network_id)`, `get_premium_customer()` |
| `events()` | App events, network scan, channel utilisation | `get_app_events(network_id, page_size, timestamp, parent)` |
| `forwards()` | Port forwards | `get_forwards(network_id, parent)`, `delete_forward(network_id, forward)` |
| `insights()` | Insights time series | `get_insights(network_id, start, end, insight_type, cadence, ..)` |
| `members()` | Network members and invites | `get_members(network_id, parent)`, `create_invite(network_id, role)`, `query_invite(invite_code)` |
| `networks()` | Networks, account, guest network, speed tests | `get_networks()`, `get_network(network_id, parent)`, `get_account()`, `reboot_network` |
| `notifications()` | Notification settings, history, push | `get_settings(network_id, parent)`, `set_push_settings(settings)` |
| `ouicheck()` | OUI check | `get_ouicheck(network_id, serial, version, parent)` |
| `permissions()` | Permissions | `get_permissions(network_id, parent)` |
| `power_saving()` | Power saving and its schedules | `set_power_saving(network_id, enable, power_saving_schedule_enabled, parent)`, `get_schedules` |
| `profiles()` | Profiles | `get_profiles(network_id, parent)`, `pause_profile(network_id, profile_id, paused, parent)` |
| `reservations()` | DHCP reservations | `get_reservations(network_id, parent)`, `create_reservation(network_id, reservation_data, parent)` |
| `routing()` | Routing (published on 2.3) | `get_routing(network_id, parent)` |
| `schedule()` | Profile pause schedules | `get_schedules(network_id, profile_id, parent)`, `create_schedule`, `delete_schedule` |
| `security()` | WPA3, band steering, UPnP, IPv6, MLO, fast transition, Passpoint, proxied nodes | `get_security_settings(network_id, parent)`, `set_upnp` |
| `sqm()` | Smart Queue Management | `get_sqm_settings(network_id, parent)`, `set_sqm(network_id, enabled, parent)` |
| `subnets()` | Subnet configuration and content filters | `get_config(network_id, parent)`, `set_config(network_id, config)`, `delete_subnet` |
| `support()` | Support | `get_support(network_id, parent)`, `request_support` |
| `thread()` | Thread network | `get_thread(network_id, parent)`, `set_thread_enabled`, `regenerate_thread_credentials` |
| `transfer()` | Transfer statistics | `get_transfer_stats(network_id, device_id, parent)` |
| `updates()` | Firmware updates | `get_updates(network_id, parent)`, `apply_update` |
| `wan()` | Multi-static IP and secondary WAN (2.3) | `get_multistaticip(network_id, parent)`, `set_secondary_wan_config` |
| `wpa3()` | Per-band WPA3 | `get_wpa3_per_band(network_id, parent)`, `set_wpa3_per_band` |

Plus `auth()` (`AuthApi`: `is_authenticated`, `session`, `logout`, `refresh_session`,
`ensure_authenticated`, `set_session_token`, `clear_session_token`, `clear_auth_data`) and
`transport()`. Several domain methods carry a shorter name than their `Client` wrapper
(`account().set_name` vs `client.set_account_name`, `entitlements().get_features` vs
`client.get_entitlement_features`, `data_usage().get_breakdown` vs
`client.get_data_usage_breakdown`, `notifications().get_settings` vs
`client.get_notification_settings`, `backup_access_points().list` vs
`client.list_backup_access_points`), exactly as in the Python library.

---

## Writes and safety

The API accepts unrecognised JSON keys with `200 OK` and discards them, so "it returned 200" is
not evidence a write applied. The crate therefore distinguishes:

* **Verified live writes** (no warning): `set_led`, `set_led_brightness`, `reboot_eero`,
  `run_speed_test`, `set_guest_network`, `set_guest_password`, `clear_guest_password`,
  `set_device_nickname`, `pause_device`, `set_device_type`, `unblock_device`.
* **Every other write** logs one fixed `WARNING` through
  `rusteero::links::warn_uncharacterised_write` immediately before the request. The message never
  includes a caller-supplied identifier.
* **Settings-class writes** say so in their warning — they may reboot every eero, as the DNS write
  path was confirmed to: every DNS write, `set_sqm`, `set_dhcp`, `set_connection_mode`,
  `set_nat_port_randomization`, `set_mlo_mode`, `set_wpa3`/`set_band_steering`/`set_upnp`/
  `set_ipv6`/`configure_security`, `set_secondary_wan_config`,
  `set_device_secondary_wan_access`, and `apply_update` (reboots every node).
* `set_device_labels` was verified upstream to return `200` and change nothing.

The discipline the warning asks for: read the current state, compare, write only on a
difference, and never retry a failed write in a loop.

```rust
let net = client.get_network(None, false).await?;
if net.data()["upnp"] != serde_json::json!(true) {
    client.set_upnp(true, None).await?;
}
```

---

## Error handling

`Error` is `#[non_exhaustive]` and mirrors `eero-api`'s exception hierarchy one-to-one. Every
server-driven variant carries `envelope: Option<Value>` (the raw parsed response) and
`error_code: Option<String>` (`meta.error`), reachable uniformly through `Error::envelope()`,
`Error::error_code()` and `Error::status()`. Messages are fixed labels — the recognised catalogue
string, or `"unrecognised error string"` — never the response body or the request URL.

| Variant | Raised when |
|---|---|
| `Authentication { message, envelope, error_code }` | Every HTTP 401 (after the refresh-and-replay attempt, when applicable); locally, `"Not authenticated"` when a call needs a session and none is configured; `LoginFlow::start`/`PendingLogin::verify` wrap other API errors as `"Login failed: …"`/`"Verification failed: …"` |
| `RateLimit { message, retry_after, envelope, error_code }` | HTTP 429, or `error.rate.limit` on any status. `retry_after` is the parsed `Retry-After` header |
| `Network(reqwest::Error)` | DNS, TCP, TLS or connect failure |
| `Timeout` | The request exceeded the configured timeout |
| `Api { status, message, envelope, error_code, url }` | Any other non-2xx status, every recognised domain string, an unrecognised string; also client-side: a refused redirect, a body over 10 MiB, invalid JSON on a 2xx |
| `AccessDenied { status, message, envelope, error_code }` | HTTP 403 with `error.access.denied`. Not an auth error; credentials kept |
| `ClientBlocked { .. }` | `error.app.version.blocked` on any status |
| `NotFound { status, message, envelope, error_code }` | Every HTTP 404, whatever `meta.error` says |
| `PremiumRequired { status: Option<u16>, .. }` | `error.premium.user_not_subscribed` or `error.partner.unavailable` on any status |
| `FeatureUnavailable { status: Option<u16>, .. }` | `error.eero.offline`, `error.network.unavailable`, `error.eero.not.capable`, … on any status |
| `Validation { field, message, envelope, error_code }` | Locally, before any request (bad id, off-host URL, invalid IP literal, bad `cadence`/mode/band, empty token, non-ASCII header); from the API, HTTP 400 with a `VALIDATION` string (`field == "request"`) |
| `MissingNetworkId` | No `network_id`, no preferred network, and no auto-discovery for this method |
| `Storage(StorageError)` | A credential-store operation failed (surfaces under `StorageFailures::Fatal`, or from `FileStore`/`KeyringStore` directly) |
| `Json(serde_json::Error)` | `Envelope::data_as::<T>()` could not deserialise `data` into `T` |

Classification precedence, ported from `eero-api`: a 401 always wins; then a string in a
status-independent group (premium, feature-unavailable, client-blocked, rate-limit) picks the
variant regardless of status; then the HTTP status picks it; everything else is `Api`.

```rust
use rusteero::{classify_error_code, Error, ErrorGroup};

match client.create_reservation(serde_json::json!({ /* … */ }), None).await {
    Ok(env) => println!("{}", env.data()),
    Err(Error::NotFound { .. }) => eprintln!("no such network"),
    Err(Error::AccessDenied { .. }) => eprintln!("this account may not do that"),
    Err(Error::Validation { field, message, .. }) => eprintln!("{field}: {message}"),
    Err(Error::Api { error_code, .. }) if error_code.as_deref() == Some("error.assignment.ip.unavailable") => {
        eprintln!("pick another address");
    }
    Err(e) if e.is_auth_error() => eprintln!("re-authenticate"),
    Err(e) => {
        eprintln!("{:?}", classify_error_code(e.error_code()));   // Option<ErrorGroup>
        return Err(e);
    }
}
```

`ErrorGroup` (`rusteero::ErrorGroup`) has the eleven catalogue groups — `Session`,
`SessionRefresh`, `Verification`, `AccessDenied`, `NotFound`, `RateLimit`, `Validation`,
`Premium`, `FeatureUnavailable`, `ClientBlocked`, `Domain` — and the member strings are the
`pub const` slices in `rusteero::errors` (`SESSION_ERRORS`, `DOMAIN_ERRORS`, …).
`Error::is_auth_error()` is `true` for `Authentication` and for any status-carrying variant with
`status == 401`; a 403 is deliberately not an auth error.

---

## Envelope

Every endpoint method returns `Envelope`, a lossless view over the raw wire JSON:

```rust
let env = client.get_account(false).await?;
env.meta().code;                 // Option<u16>
env.meta().error;                // Option<String> — e.g. "error.session.refresh"
env.meta().extra;                // any other meta keys, verbatim
env.data();                      // &serde_json::Value, or &Value::Null if "data" is absent
env.as_value();                  // &Value: the whole envelope, borrowed
env.data_as::<MyAccount>()?;     // deserialise `data` into your own serde type
env.into_value();                // the exact JSON value this envelope was built from
```

`data_as::<T>()` is the crate's only source of `Error::Json` — it means `T`'s shape didn't match
`data`, not that the response was malformed (a malformed 2xx body is already `Error::Api` before
an `Envelope` exists). `Envelope`'s `Debug` prints only `meta.code` and the shape of `data`,
since a cached response can carry a Wi-Fi password.

---

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `keyring` | on | Enables `KeyringStore` (and the keyring rows of `create_storage`), backed by the OS keyring |
| `typed` | off | Reserved; currently a no-op. Use `Envelope::data_as::<T>()` with your own types |

---

## 🔗 Related Pages

*   [⚙️ Configuration](Configuration)
*   [🔧 Troubleshooting](Troubleshooting)
*   [🔀 Migration](Migration)
*   [🏠 Home](Home)
