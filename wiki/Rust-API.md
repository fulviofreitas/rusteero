# 🦀 Rust API

Full reference for `rusteero`'s public surface. Method names are the Python
[`eero-api`](https://github.com/fulviofreitas/eero-api) names verbatim, in `snake_case`
(including the `get_` prefix) — deliberate, so a method can be searched for across both
libraries by name. Every endpoint method returns `Result<Envelope, rusteero::Error>` over the
raw `{"meta": …, "data": …}` wire payload; nothing is transformed on the way through.

`Client` has roughly 100 public methods, one per read or write `eero-api`'s `EeroClient`
exposes (plus a handful this port adds where `EeroApi` already had the underlying call). This
page covers construction, authentication, caching, a representative slice of the endpoint
surface grouped by domain, and error handling — see [PARITY.md](https://github.com/fulviofreitas/rusteero/blob/master/PARITY.md)
in the repository for the exhaustive method-by-method table.

## Quick Start

```rust
use rusteero::auth::Session;
use rusteero::Client;

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

## Client construction

```rust
use rusteero::auth::Session;
use rusteero::storage::MemoryStore;
use rusteero::transport::StorageFailures;
use rusteero::Client;
use std::sync::Arc;
use std::time::Duration;

// `session` is a `Session` obtained earlier, e.g. from `PendingLogin::verify` or
// `Session::from_token`.
let client = Client::builder()
    .session(Some(session))                             // Option<Session> — omit/None to load from the store
    .store(Some(Arc::new(MemoryStore::new())))           // Option<Arc<dyn CredentialStore>> — None means no persistence
    .storage_failures(StorageFailures::Warn)             // Warn (default) | Fatal
    .cache_ttl(Duration::from_secs(60))                  // Duration::ZERO disables cache reads (writes still happen)
    .user_agent(Some("my-tool/1.0".to_owned()))          // default: reqwest's own User-Agent
    .http(reqwest::Client::new())                        // optional custom HTTP client (see the warning below)
    .build()
    .await?;
```

Every setter takes `self` by value and returns `Self`; `build()` is the only fallible, only
`async` step. If both `.session(..)` and `.store(..)` are set, the explicit session wins — the
store is still installed (so `set_session_token`/`clear_session_token`/`logout` persist through
it), but it is only *read* at build time when no explicit session was given.

Passing a custom `reqwest::Client` via `.http(..)` silently discards two safety guarantees this
crate relies on: redirect refusal and the built-in request/read timeouts. Build your own client
with `.redirect(reqwest::redirect::Policy::none())` and explicit timeouts first if you need both
custom configuration and these guarantees.

`EeroApi` (no cache, no network-id auto-resolution) is available for a caller who wants the
lowest layer directly:

```rust
let raw = client.api().networks().get_network("123").await?;
```

---

## Authentication

### Interactive login flow (separable from the client)

The one-time-code handshake is a type-state pair, `LoginFlow` → `PendingLogin` → `Session`, and
needs neither a `Client` nor a `CredentialStore`:

```rust
use rusteero::auth::flow::LoginFlow;

let pending = LoginFlow::new(None)?.start("you@example.com").await?;  // email or phone
pending.resend().await?;                                              // optional: ask for a new code
let session = pending.verify("123456").await?;                        // consumes `pending` → Session
```

`LoginFlow::new(None)` builds a transport against the real Eero cloud hosts; pass
`Some(reqwest::Client)` to supply your own. There is no `Client::login`/`Client::verify` — an
unverified login token can never reach an authenticated endpoint by construction, since only
`PendingLogin::verify` can ever produce a `Session`.

### Headless / CI: inject a token directly

```rust
use rusteero::auth::Session;

let session = Session::from_token("s-...");                          // fabricates a 30-day expiry
let session = Session::from_env("RUSTEERO_SESSION_TOKEN")?;          // reads the token from an env var
let client = Client::builder().session(Some(session)).build().await?;
```

### Check / refresh / logout

```rust
client.is_authenticated();                            // local check only: token present, not expired
client.api().auth().refresh_session().await?;         // POST login/refresh, falling back to account/refresh
client.logout().await?;                                // POST logout; clears the session, the store and this client's cache regardless of outcome
client.set_session_token("s-...")?;                    // inject a token without the interactive step; clears cache
client.clear_session_token()?;                          // clears the token, keeps any refresh token; clears cache
```

`refresh_session` lives on `AuthApi`, reachable via `client.api().auth()` — `Client` itself has
no `refresh_session` wrapper. In practice a normal login/verify never receives a refresh token
from the server, so this is close to a no-op; it is implemented for parity with `eero-api`.

---

## Cached getters & network-id resolution

Eight methods — `get_account`, `get_networks`, `get_network`, `get_eeros`, `get_devices`,
`get_device`, `get_profiles`, `get_profile` — are backed by an in-memory TTL cache (default 60
seconds, `ClientBuilder::cache_ttl`). Every one of them takes a trailing `refresh_cache: bool`:
`true` skips the cache read but still writes the fresh response back.

```rust
let networks = client.get_networks(false).await?;         // GET /networks, falling back to /account if empty
let network  = client.get_network(None, false).await?;    // None → resolve a network id (see below)
let devices  = client.get_devices(None, false).await?;
let device   = client.get_device("device-id", None, false).await?;   // item id first, network_id second
```

Every method that needs a network id takes `network_id: Option<&str>` and resolves it in this
order:

1. The explicit `network_id`, if non-empty.
2. `Client::preferred_network_id()`, set via `client.set_preferred_network("123")` — in-memory
   only, lost on restart, never synced with the server.
3. For most methods, auto-discovery: the first network from `get_networks(false)`.
4. Otherwise, `Error::MissingNetworkId`.

A handful of "settings"-style methods (everything from `get_diagnostics` onward in the source,
e.g. `get_dns_settings`, `run_diagnostics`, the DNS/SQM/security setters) skip step 3 and go
straight to `Error::MissingNetworkId` if no id was resolved by step 2 — this matches `eero-api`'s
own behaviour method-for-method; see each method's doc comment in `src/client.rs` for which
group it falls into.

**A quirk worth knowing before you rely on it:** `get_dns_settings`, `get_security_settings`,
`get_sqm_settings` and `get_premium_status` all return the *whole* network object — byte-identical
to `get_network` — not a filtered view. Read the specific keys you want out of `data` yourself.
This was confirmed against a real account, not just inferred from the Python source (see
[PARITY.md](https://github.com/fulviofreitas/rusteero/blob/master/PARITY.md)'s "Live validation"
section).

`get_networks()` also has a real fallback path: if `GET /networks` comes back with an empty list,
it transparently fetches `GET /account` and synthesizes a response from `data.networks` there
instead. On the one real account this was validated against, `/networks` came back empty and the
`/account` fallback is what actually resolved anything — it is load-bearing, not a vestigial
branch.

---

## Endpoint surface, by domain

`Client` wraps every read and write with cache handling and network-id resolution.
`client.api()` exposes the same 25 domain modules with neither, for a caller who wants raw
control (or a network id it already knows is valid).

### Networks & guest network

```rust
client.set_preferred_network("123");
client.set_network_name("Home", Some("123")).await?;
client.reboot_network(Some("123")).await?;
client.run_speed_test(None).await?;

// set_guest_network(enabled, name, password, network_id)
client.set_guest_network(true, Some("Guests"), Some("s3cret"), None).await?;
client.set_guest_network(false, None, None, None).await?;
```

### Eero devices (mesh nodes)

```rust
let eeros = client.get_eeros(None, false).await?;
client.reboot_eero("eero-id", None).await?;
client.set_led("eero-id", false, None).await?;
client.set_led_brightness("eero-id", 40, None).await?;         // clamped 0-100 server-side
client.set_nightlight("eero-id", Some(true), None, None, None, None, None, None).await?;
```

### Connected clients (devices)

```rust
let devices = client.get_devices(None, false).await?;
let device  = client.get_device("device-id", None, false).await?;
client.set_device_nickname("device-id", "Laptop", None).await?;   // writes go to API 2.3
client.pause_device("device-id", true, None).await?;               // writes go to API 2.3
client.block_device("device-id", true, None).await?;               // see "block_device" below
```

**Device writes go to API version 2.3, not 2.2.** Nickname and pause writes sent to 2.2 return
`200 OK` but silently never persist server-side (`eero-api` issue #102) — every device-mutating
call in this crate is routed to `/2.3` for exactly this reason.

**`block_device` performs up to two round trips.** Blocking a device (`blocked: true`) first
issues a `GET` to resolve the device's MAC address, then a `POST /blacklist` with that MAC.
Unblocking (`blocked: false`) is a single `DELETE /blacklist/{id}`. `PUT {"blocked": …}` is *not*
how blocking works on this API — it is a silent no-op (`eero-api` issue #109) — which is why this
method exists instead of a plain settings write. If the `GET` succeeds but the device has no
usable MAC, this returns `Error::Api { status: 502, .. }` (a client-fabricated status; the server
was never asked).

### Profiles

```rust
let profiles = client.get_profiles(None, false).await?;
client.pause_profile("pid", true, None).await?;
client.create_profile("Kids", None).await?;
client.set_profile_devices("pid", &["/2.2/networks/123/devices/abc"], None).await?;
client.update_profile_content_filter("pid", &[("safe_search", true)], None).await?;
client.set_blocked_applications("pid", &["tiktok"], None).await?;
```

### Schedules

```rust
client.enable_bedtime("pid", "21:00", "07:00", None, None).await?;   // days: Option<&[&str]>
client.set_weekday_bedtime("pid", "21:00", "07:00", None).await?;
client.clear_profile_schedule("pid", None).await?;
```

### Speed tests & diagnostics

```rust
client.run_speed_test(None).await?;
client.run_diagnostics(None).await?;
client.get_diagnostics(None).await?;
```

### Settings writers (DNS / security / SQM)

```rust
client.set_dns_mode("cloudflare", None, None).await?;   // "cloudflare" | "google" | "opendns" | "custom" | "auto"
client.set_wpa3(true, None).await?;
client.set_sqm_enabled(true, None).await?;
```

`set_dns_mode`'s `mode` is a plain `&str`, validated server-side of the call (there is no enum);
an unrecognised value returns `Error::Validation`. `"custom"` requires `custom_servers` to be
`Some` and non-empty.

---

## Error handling

```rust
match client.get_network(None, false).await {
    Ok(env) => println!("{}", env.data()),
    Err(rusteero::Error::Authentication(msg)) => eprintln!("re-login: {msg}"),
    Err(rusteero::Error::RateLimit { retry_after }) => eprintln!("slow down: {retry_after:?}"),
    Err(rusteero::Error::Api { status, message, .. }) => eprintln!("API {status}: {message}"),
    Err(e) if e.is_auth_error() => eprintln!("auth problem"),
    Err(e) => return Err(e),
}
```

`Error` is `#[non_exhaustive]`. Variants mirror the Python exception hierarchy one-to-one:
`Authentication`, `RateLimit { retry_after }`, `Network`, `Api { status, message, url }`,
`Timeout`, `NotFound`, `PremiumRequired`, `FeatureUnavailable`, `Validation { field, message }`,
`MissingNetworkId`, plus two additions with no Python equivalent, `Storage` and `Json`.
`NotFound`, `PremiumRequired` and `FeatureUnavailable` are kept for 1:1 parity but never actually
constructed anywhere in this crate — a real "not found" always surfaces as `Api { status: 404,
.. }` instead.

`Error::is_auth_error()` is `true` for `Authentication` unconditionally and for `Api` iff
`status == 401` — a `403` is deliberately *not* treated as an auth error (it usually means
"insufficient subscription tier", not "not logged in").

---

## Envelope

Every endpoint method returns `Envelope`, a lossless view over the raw wire JSON:

```rust
let env = client.get_account(false).await?;
env.meta().code;                 // Option<u16>
env.meta().error;                // Option<String> — e.g. "error.session.refresh"
env.data();                      // &serde_json::Value, or &Value::Null if "data" is absent
env.data_as::<MyAccount>()?;     // deserialize `data` into your own serde type
env.into_value();                // the exact JSON value this envelope was built from
```

`data_as::<T>()` is the crate's only source of `Error::Json` — it means `T`'s shape didn't match
`data`, not that the response itself was malformed (a malformed 2xx body is already mapped to
`Error::Api` before an `Envelope` ever exists).

---

## Advanced: custom HTTP client / base URL

```rust
let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()?;
let client = Client::builder()
    .http(http)
    .base_url("http://127.0.0.1:8080")   // derives both /2.2 and /2.3 from one root — useful in tests
    .build()
    .await?;
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `keyring` | on | Enables `KeyringStore`, backed by the OS keyring |
| `typed` | off | Reserved for future typed response models. Currently a no-op — there is no `rusteero::models` module yet; use `Envelope::data_as::<T>()` with your own types today |

---

## 🔗 Related Pages

*   [⚙️ Configuration](Configuration)
*   [🔧 Troubleshooting](Troubleshooting)
*   [🏠 Home](Home)
