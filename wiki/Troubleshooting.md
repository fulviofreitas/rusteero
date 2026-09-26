# 🔧 Troubleshooting

## Authentication

### Amazon Login Accounts

If your Eero account uses Amazon for login (Sign in with Amazon), this library cannot complete
the email/SMS one-time-code handshake directly — Amazon-linked accounts don't have a code to
verify against `POST login/verify`.

**Workaround:**

1.  Have someone in your household create a standard Eero account using email and password (not Amazon login)
2.  In the Eero app, invite that account as an admin to your network
3.  Use those new credentials to authenticate with this library

```rust
use rusteero::auth::flow::LoginFlow;

// Use the new email/password account, not your Amazon-linked account
let pending = LoginFlow::new(None)?.start("new-admin@example.com").await?;
let session = pending.verify(&code).await?;
```

> **Note**: The invited admin account will have full access to manage the network.

### 401: `error.session.expired` vs `error.session.refresh`

Every HTTP 401 surfaces as `Error::Authentication { error_code, .. }`, but two of the catalogue
strings mean very different things:

| `error_code()` | What happened | What to do |
|---|---|---|
| `error.session.refresh` | The server asked for a refresh. The transport already called `POST login/refresh` with the current token and replayed your request once; you only see this error if that refresh **failed** | Nothing periodic — the crate handles it. If you see it, the refresh endpoint rejected the token: re-seed one or run `LoginFlow` again |
| `error.session.expired`, `error.session.invalid`, `error.session.revoked` | Terminal. If it came from the refresh endpoint, the stored credential has **already been cleared** (in memory and in every configured store) | Re-seed a token (`set_session_token`, `Session::from_env`) or run the login flow |
| `error.verification.required` and the other `Verification` strings | The account is mid-verification or blocked from completing login; credentials are kept | Finish the one-time-code flow |
| `None` / free text | A 401 with no recognised string | Treat as terminal; `Error::envelope()` has the API's own body |

```rust
use rusteero::{classify_error_code, Error, ErrorGroup};

if let Err(Error::Authentication { error_code, .. }) = client.get_devices(None, false, None, None).await {
    match classify_error_code(error_code.as_deref()) {
        Some(ErrorGroup::Verification) => { /* finish the OTP flow */ }
        _ => { /* re-seed a token or re-run LoginFlow; the store is already cleared */ }
    }
}
```

There is no client-side expiry: `client.is_authenticated()` only says a token is configured.
A long-running process can hold one token indefinitely; catch the terminal
`Error::Authentication`, re-authenticate, continue — do not add your own periodic
`refresh_session()` calls.

### `"Not authenticated"` before any request

`Error::Authentication` with the message `"Not authenticated"` and no `error_code` is the local
guard: the `Client` has no session. Either `.session(..)` was never set and the store was empty,
or `logout()`/`clear_session_token()` ran. Under the default `StorageFailures::Warn` a store
that failed to load is only logged — check the `WARN` line.

### `Error::MissingNetworkId`

Returned when a method needs a network id, none was passed, no preferred network was set via
`client.set_preferred_network(..)`, and the method is not one of the 24 that auto-discover (see
[Rust API — Network targeting](Rust-API#network-targeting-network_id-optionstr)). Every
settings-class read and write, and every family new in v8, is in the non-discovering group.
Call `client.get_networks(false)` once early — it sets the preferred network as a side effect —
or pass `Some("<network-id>")`.

### Keyring unavailable (headless Linux, containers)

`KeyringStore` returns `StorageError::Backend { backend: "keyring", .. }` when no Secret Service
(or equivalent) is running. With the default `StorageFailures::Warn` the in-memory session
still works for the lifetime of the process — only persistence fails, logged at `WARN`. Options:

*   Chain a file fallback: `ChainedStore::new(Arc::new(KeyringStore::new()), Arc::new(FileStore::new(path)))`
    — a failed keyring save now genuinely falls through to the file.
*   Use `FileStore` or `MemoryStore` directly, or `StorageConfig { use_keyring: false, .. }`.
*   Build with `default-features = false` to drop the `keyring` dependency entirely (see
    [Configuration](Configuration#disabling-the-keyring-for-headless-use)).

## API responses

### `Error::PremiumRequired` / `Error::FeatureUnavailable`

These are status-independent: the API can carry `error.premium.user_not_subscribed` or
`error.eero.offline` on a `200`, a `400` or a `500`, and the crate maps the string, not the
status. `PremiumRequired` means the network is not on Eero Plus/Secure — the DNS-policies
family (`get_advanced_content_filter`, `block_domain*`, `set_profile_blocked_applications`,
…) and `get_premium_status`-adjacent features are the usual sources. `FeatureUnavailable`
covers an offline eero, a node that is not capable of the feature, a deactivated or
organisation-owned unit; `set_nightlight` on an eero without a nightlight is the common one.
Both carry `status: Option<u16>` and the raw `envelope()`.

### `Error::Validation` before any request was sent

A `Validation { field, .. }` with no `envelope()` came from a local check. The common ones:

| `field` | Cause |
|---|---|
| `"id"`, `"id_or_url"`, `"url"`, `"link"`, `"path"`, `"child"` | An id that is not a single path segment, a `..`, an absolute URL on another host or scheme, a child path that does not belong to the addressed network |
| `"cadence"` | Not `"daily"`/`"hourly"` (data usage) or `"hourly"`/`"daily"` (insights) — `rusteero::params::CADENCE_VALUES` |
| `"mode"` | `set_dns_mode` given anything but `auto`/`automatic`/`custom` (provider presets are gone); `set_dhcp` given anything but `automatic`/`manual`; `set_connection_mode` not `BRIDGE`/`NAT`; `set_mlo_mode` not `disabled`/`multi`/`single` |
| `"family"` | `clear_custom_dns` given anything but `None`, `"ipv4"`, `"ipv6"` |
| `"dns_servers"` and friends | A malformed IP literal, an address of the wrong family for the method, a zone-scoped address, more than 2 servers for one family, or an empty list where one is required |
| `"band_2_4_ghz"` / `"band_5_ghz"` / `"wpa3_per_band"` | A value outside `WPA2`/`WPA2_WPA3`/`WPA3`, or both bands `None` |
| `"serial"` / `"version"` | `get_ouicheck` given an empty value |
| `"role"` | `create_invite` given anything but `admin`/`owner` |
| `"token"` / the env-var name | An empty or non-printable-ASCII session token |
| `"base_url"`, `"X-Accept-Language"`, `"user_agent"` | Builder inputs rejected at `build()` |

A `Validation` **with** an `envelope()` and `field == "request"` is the API's own 400 form error
(`error.form.errors`, `error.reservation.ip.invalid`, …); read `error_code()` and the envelope.

### The write returned 200 and nothing changed

The API accepts unrecognised JSON keys with `200 OK` and discards them, so a `2xx` is not
evidence a write applied. Three things to check:

1.  **Was it a warned write?** Every write not verified live logs one `WARNING` first
    ("Issuing write (…): its side effects have not been fully characterised …"). Read the
    resource back; if the value is unchanged, the API declares a different form for that
    operation than the one the Python library reverse-engineered. `set_device_labels` is the
    known case — verified upstream to echo the labels back and never apply them.
2.  **Settings-class writes reboot the mesh.** DNS, DHCP, connection mode, NAT randomisation,
    MLO, the security quartet, SQM, secondary WAN and `apply_update` say so in their warning. A
    read-back immediately after may still show the old value while the eeros restart; do not
    retry in a loop.
3.  **Device writes must go to API 2.3.** `set_device_nickname` and `pause_device` already do; the
    same `PUT` on `2.2` returns `200` and is dropped server-side. `update_device_via_link` is a
    JSON `PUT` to the device's own URL on `2.2` and is unverified — prefer the two verified
    methods. Blocking is `block_device` (`POST /blacklist` with the device's MAC) and
    `unblock_device` (`DELETE /blacklist/{mac}`); a `PUT {"blocked": …}` has never worked.

### `Error::NotFound` — and 2.2 vs 2.3

Every HTTP 404 is `Error::NotFound { status: 404, .. }`, whether or not `meta.error` carries a
recognised string. Two families 404 by design rather than by mistake:

*   `get_multistaticip` (API 2.3) returns `NotFound` on a network without the multi-static-IP
    feature (`error.network.multistaticip_not_found`).
*   `get_ouicheck` returns 404 unless both `serial` and `version` are supplied — which is why they
    are required arguments.

If a resource you can see in the app 404s on a bare id, pass the resource's own `url` (or the
whole envelope as `parent` on the domain API) instead: some resources are published on `2.3`
(`routing`, secondary WAN, multi-static IP) while the bare-id template defaults to `2.2`. The
`Client` wrappers do this automatically while a fresh `get_network()` result is cached.

### `Error::AccessDenied` on `get_invites` and friends

A 403 with `error.access.denied` is `AccessDenied`, not an auth error — the session is valid,
the account simply may not do that on this network (`get_invites` is access-denied on some
accounts, `get_transfer_stats` on others). `is_auth_error()` is `false`; credentials are kept.

### No networks found

`get_networks()` falls back to `GET /account` and reads `data.networks` when the direct
`/networks` list comes back empty — this has been observed on a real account where `/networks`
returned nothing and the fallback is what worked. If both are empty, the account genuinely has
no networks. Note that auto-discovery reads `get_networks(false)`, so a cached empty answer
keeps discovery failing for one TTL window — call `get_networks(true)` to force it.

### Rate limited

`Error::RateLimit { retry_after, .. }` — back off for `retry_after` if the server sent a
`Retry-After` header (`None` otherwise). The crate never retries a rate-limited request, and
`get_retries` does not apply to `429`.

### Timeouts

Defaults: 30 s total, 10 s per read (`consts::REQUEST_TIMEOUT` / `consts::READ_TIMEOUT`). A
`GET` that times out is retried only if `get_retries` is above 0. To change the timeouts,
supply your own `reqwest::Client` via `.http(..)` — and rebuild redirect refusal as well
(`.redirect(reqwest::redirect::Policy::none())`), since a caller-supplied client discards both.

## DNS writes

**These take effect.** Before `eero-api` 7.0 (and in `rusteero` 1.0.0, which ported that
shape) the DNS writers sent a `custom_dns` field the API does not have, so they returned `200`
and changed nothing. The v8 port sends the real shape, and **every DNS write reboots every
eero**. Audit any unattended code that calls `set_custom_dns`, `set_custom_dns_ipv4`,
`set_custom_dns_ipv6`, `clear_custom_dns`, `set_dns_mode` or `set_dns_caching` before upgrading.

What the current methods do:

*   `set_custom_dns(&["1.1.1.1", "1.0.0.1", "2606:4700:4700::1111"], None)` splits the list by
    address family and switches each represented family to custom mode; a family not in the
    list is left untouched. Max **2 servers per family**; more is `Error::Validation`.
*   `set_custom_dns_ipv4` / `set_custom_dns_ipv6` address one family and leave the other alone.
*   `clear_custom_dns(None | Some("ipv4") | Some("ipv6"), nid)` switches the family back to
    `automatic` and **retains** the stored servers.
*   `set_dns_mode("custom", None, nid)` re-enables whatever servers the network already stores —
    the inverse of `clear_custom_dns`. `set_dns_mode("auto" | "automatic", ..)` is `clear_custom_dns(None)`.
*   Provider presets (`"cloudflare"`, `"google"`, `"opendns"`) are **not** accepted — read the
    API's own catalogue instead:

```rust
let dns = client.get_dns_settings(None).await?;
let servers = dns.data()["dns"]["default_test_servers"]
    .as_array()
    .into_iter()
    .flatten()
    .find(|p| p["name"] == "Cloudflare")
    .expect("catalogue entry");
// each entry has "name", "ipv4" and "ipv6" lists
```

Reading DNS settings — `get_dns_settings` returns the whole network object; the real paths are:

```text
data.dns.mode                       "custom" | "automatic"
data.dns.custom.ips                 IPv4 servers
data.dns.caching                    bool
data.ipv6.name_servers.mode         "custom" | "automatic"
data.ipv6.name_servers.custom       IPv6 servers, fully expanded
```

Note the asymmetry (`custom.ips` vs `custom`), and that IPv6 addresses read back expanded
(`2606:4700:4700::1111` becomes `2606:4700:4700:0:0:0:0:1111`) — compare with
`std::net::Ipv6Addr`, not string equality. There is no `custom_dns`, `dns_caching` or
`dns_servers` key anywhere in the response.

## Debug logging

```rust
tracing_subscriber::fmt()
    .with_env_filter("rusteero=debug")
    .init();
```

Logs contain method, path and status only; an error response's envelope is logged at `DEBUG`
after redaction. Headers, cookies and raw bodies are never logged, and error messages never
embed a response body — read `Error::envelope()` instead. See
[Configuration — Logging and redaction](Configuration#logging-and-redaction).

---

## 🔗 Related Pages

*   [📖 Rust API](Rust-API)
*   [⚙️ Configuration](Configuration)
*   [🔀 Migration](Migration)
*   [🏠 Home](Home)
