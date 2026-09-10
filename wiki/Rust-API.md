# 🦀 Rust API

> 🚧 **Phase 0.** Signatures below are the planned public surface (see `PARITY.md`); they are
> filled in with real, compiled examples as each phase lands. Names match the Python
> [`eero-api`](https://github.com/fulviofreitas/eero-api/wiki/Python-API) methods one-to-one.

## Quick Start

```rust
use rusteero::{Client, Session};

let client = Client::builder()
    .session(Session::from_env("RUSTEERO_SESSION_TOKEN")?)
    .build()?;
let account = client.get_account(false).await?;
println!("{}", account.into_value());          // exact wire JSON
```

---

## Client construction

```rust
let client = rusteero::Client::builder()
    .session(session)                          // Session — or omit to load from the store
    .store(rusteero::storage::MemoryStore::new())   // any CredentialStore (default: memory)
    .storage_failures(rusteero::StorageFailures::Warn) // Warn (default) | Fatal
    .cache_ttl(std::time::Duration::from_secs(60))     // Duration::ZERO disables reads
    .user_agent("my-tool/1.0")                 // default: reqwest's default UA
    .http(reqwest::Client::new())              // optional custom HTTP client
    .build()?;
```

`EeroApi` (no cache, no network-id auto-resolution) is available for consumers who want the
lowest layer: `client.api().networks().get_network("123")`.

---

## Authentication

### Login flow (interactive, separable from the client)

```rust
use rusteero::auth::LoginFlow;

let pending = LoginFlow::new(None).start("you@example.com").await?;   // email or phone
pending.resend().await?;                                              // optional
let session = pending.verify("123456").await?;                        // → Session (30-day expiry)
```

### Headless / CI

```rust
let session = rusteero::Session::from_token("s-…");
let session = rusteero::Session::from_env("RUSTEERO_SESSION_TOKEN")?;
```

### Check / refresh / logout

```rust
client.is_authenticated();          // local check: token present and not expired
client.refresh_session().await?;    // POST login/refresh, then account/refresh
client.logout().await?;             // POST logout + clears the store
client.set_session_token("s-…")?;   // inject without the interactive step
client.clear_session_token();
```

---

## Networks

```rust
let nets = client.get_networks(false).await?;              // GET networks (falls back to /account)
let net  = client.get_network(None, false).await?;         // None → preferred / first network
client.set_preferred_network("123");
client.set_network_name(Some("123"), "Home").await?;       // PUT networks/{id}/settings
client.reboot_network(Some("123")).await?;
```

## Guest Network

```rust
client.set_guest_network(None, true, Some("Guests"), Some("s3cret")).await?;
client.set_guest_network(None, false, None, None).await?;
```

## Eero Devices (Mesh Nodes)

```rust
let eeros = client.get_eeros(None, false).await?;
client.reboot_eero(None, "eero-id").await?;
client.set_led(None, "eero-id", false).await?;
client.set_led_brightness(None, "eero-id", 40).await?;     // clamped 0–100
client.set_nightlight(None, "eero-id", NightlightUpdate { enabled: Some(true), ..Default::default() }).await?;
```

## Connected Clients (Devices)

```rust
let devices = client.get_devices(None, false).await?;
let device  = client.get_device(None, "device-id", false).await?;
client.set_device_nickname(None, "device-id", "Laptop").await?;   // PUT /2.3/…
client.pause_device(None, "device-id", true).await?;               // PUT /2.3/…
client.block_device(None, "device-id", true).await?;               // POST …/blacklist
```

## Profiles

```rust
let profiles = client.get_profiles(None, false).await?;
client.pause_profile(None, "pid", true).await?;
client.create_profile(None, "Kids").await?;
client.set_profile_devices(None, "pid", &["/2.2/networks/123/devices/abc"]).await?;
client.update_profile_content_filter(None, "pid", &[("safe_search", true)]).await?;
client.set_blocked_applications(None, "pid", &["tiktok"]).await?;
```

## Schedules

```rust
client.enable_bedtime(None, "pid", "21:00", "07:00", None).await?;
client.set_weekday_bedtime(None, "pid", "21:00", "07:00").await?;
client.clear_profile_schedule(None, "pid").await?;
```

## Speed Tests & Diagnostics

```rust
client.run_speed_test(None).await?;
client.run_diagnostics(None).await?;
client.get_diagnostics(None).await?;
```

## Settings writers (DNS / Security / SQM)

```rust
client.set_dns_mode(None, DnsMode::Cloudflare, None).await?;
client.set_wpa3(None, true).await?;
client.set_sqm_enabled(None, true).await?;
```

---

## Error Handling

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

Variants mirror the Python exceptions: `Authentication`, `RateLimit`, `Network`, `Api`,
`Timeout`, `NotFound`, `PremiumRequired`, `FeatureUnavailable`, `Validation`,
`MissingNetworkId`, plus `Storage` and `Json`.

## Envelope

```rust
let env = client.get_account(false).await?;
env.meta().code;                 // Option<u16>
env.data();                      // &serde_json::Value
env.data_as::<MyAccount>()?;     // your own serde type
env.into_value();                // exact wire JSON
```

## Advanced: custom HTTP client / base URL

```rust
let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build()?;
let client = Client::builder().http(http).base_url("http://127.0.0.1:8080").build()?; // tests
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `keyring` | on | `KeyringStore` backed by the OS keyring |
| `typed` | off | `rusteero::models::*` typed views (later phase) |

---

## 🔗 Related Pages

*   [⚙️ Configuration](Configuration)
*   [🔧 Troubleshooting](Troubleshooting)
*   [🏠 Home](Home)
