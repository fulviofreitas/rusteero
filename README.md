<div align="center">

# 🦀 rusteero

**Async Rust client for the Eero mesh Wi-Fi cloud API**

[![Rust](https://img.shields.io/badge/rust-1.96%2B-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-22c55e?style=for-the-badge)](LICENSE)

---

_`rusteero` is a Rust client library for the Eero (Amazon) mesh Wi-Fi cloud API._
_It is a port of the Python [`eero-api`](https://github.com/fulviofreitas/eero-api) library at **v8.0.4**: same raw JSON contract, same method names, same credential record._

[Status](#-status) · [Quick Start](#-quick-start) · [Docs](#-docs) · [Acknowledgments](#-acknowledgments) · [License](#-license)

</div>

---

## ⚡ Why rusteero?

- 🚀 **Async-first** — `tokio` + `reqwest` (rustls), no Python runtime
- 📦 **Raw JSON** — every call returns the exact `{meta, data}` envelope Eero sends, wrapped in a lossless `Envelope`
- 🔗 **Link-aware** — every resource argument accepts a bare id, an API path or an absolute API-host URL, and every domain method takes a `parent` envelope so the request follows the link the API published
- 🔐 **Secure** — session tokens are `SecretString`s sent only to the configured API host, redirects are refused, error messages never embed a response body
- 🔁 **Compatible** — the `{"session_id", "schema_version": 2}` credential record is shared with `eero-api`, so a login made by [`eeroctl`](https://github.com/fulviofreitas/eeroctl) works unchanged (legacy records are migrated on first load)
- 🧩 **Headless-friendly** — the email/SMS code step is separable; inject a pre-obtained token instead

## 🚧 Status

**Ported to `eero-api` 8.0.4, not yet released.** Transport, authentication, credential storage,
the error catalogue, link resolution, all 37 domain modules, the `Client` facade and its cache are
in place, each pinned by wiremock tests. [`PARITY.md`](PARITY.md) is the method-by-method table
against the Python library at tag `v8.0.4`: 267 rows — 218 ported, 22 changed with a reason,
20 dropped with a reason, 3 identical, 4 renamed; none planned.

## 📦 Install

```toml
[dependencies]
rusteero = "2"

# Headless / embedded (no system keyring):
rusteero = { version = "2", default-features = false }
```

## 🚀 Quick Start

### Build a `Client`

```rust
use rusteero::{Client, Session};

#[tokio::main]
async fn main() -> Result<(), rusteero::Error> {
    // Headless: a token you already hold (e.g. from a previous login, or from eeroctl).
    let session = Session::from_env("RUSTEERO_SESSION_TOKEN")?;

    // Or interactively (once): email or phone -> one-time code -> Session.
    // use rusteero::auth::flow::LoginFlow;
    // let pending = LoginFlow::new(None)?.start("you@example.com").await?;
    // let session = pending.verify(code.trim()).await?;   // consumes `pending`

    let client = Client::builder().session(Some(session)).build().await?;

    // A cached read: `false` = serve from the 60 s cache when fresh.
    let networks = client.get_networks(false).await?;
    for n in networks.data()["networks"].as_array().into_iter().flatten() {
        println!("📶 {}: {}", n["name"], n["status"]);
    }
    Ok(())
}
```

`ClientBuilder::build()` is `async` because it may load a session from a configured
credential store. A `Client` persists nothing unless you attach one — the system keyring
(default `keyring` feature) uses the *same* entry the Python library does. See
[Configuration](https://github.com/fulviofreitas/rusteero/wiki/Configuration).

### A write: read, compare, skip

Writes are only issued after a read shows the value actually differs. Most writes have not been
characterised against a live network and log one `WARNING` before the request; a
settings-class write such as `set_sqm` may reboot the whole mesh.

```rust
use serde_json::json;

let current = client.get_sqm_settings(None).await?;      // whole network object
if current.data()["sqm"] != json!(true) {
    client.set_sqm(true, None).await?;                   // PUT ?sqm=true to the settings link
}
```

### Ids, paths, URLs and `parent`

Wherever a method takes a `network_id`, `eero_id`, `device_id` or `profile_id`, you may pass a
bare id, the resource's API path (`/2.2/networks/<id>`) or its absolute URL on the API host;
anything on another host or scheme is rejected with `Error::Validation` before a request is sent.
Every method on the domain modules (`client.api().networks()`, …) also takes
`parent: Option<&Value>` — the envelope you already hold — and resolves its URL from the link the
API published on it; `Client` passes its cached network, eero and device envelopes as `parent`
automatically.

### Error handling

```rust
use rusteero::{Error, ErrorGroup, classify_error_code};

match client.get_network(None, false).await {
    Ok(env) => println!("{}", env.data()),
    Err(Error::Authentication { error_code, .. }) => {
        // Terminal: session expired / invalid / revoked. Re-seed a token or re-run LoginFlow.
        eprintln!("re-login needed: {error_code:?}");
    }
    Err(Error::PremiumRequired { .. }) => eprintln!("needs Eero Plus"),
    Err(Error::RateLimit { retry_after, .. }) => eprintln!("back off: {retry_after:?}"),
    Err(e) => {
        // Branch on the catalogue string, never on the message text.
        if classify_error_code(e.error_code()) == Some(ErrorGroup::Domain) {
            eprintln!("domain error {:?}: {:?}", e.error_code(), e.envelope());
        }
        return Err(e);
    }
}
```

`Error::error_code()` is `meta.error` as the API sent it; `Error::envelope()` is the raw parsed
response; `Error::status()` the HTTP status when one applies. Messages are fixed, leak-safe labels.

## 📄 Raw Response Format

All API methods return the exact JSON from Eero's API, wrapped in a lossless `Envelope`:

```json
{
  "meta": { "code": 200, "server_time": "..." },
  "data": { "...": "endpoint-specific payload" }
}
```

`env.meta()`, `env.data()`, `env.into_value()` and `env.data_as::<T>()` are the only helpers.

## 🎛️ Features

| Feature | Default | Effect |
|---|---|---|
| `keyring` | on | Enables `KeyringStore`, backed by the OS keyring (macOS Keychain, Secret Service, Windows Credential Manager) |

## 📚 Docs

| Guide | What's inside |
|-------|---------------|
| **[📖 Rust API](../../wiki/Rust-API)** | Layers, builder options, every domain accessor, caching, errors |
| **[⚙️ Configuration](../../wiki/Configuration)** | Sessions, credential stores, transport options, logging |
| **[🔧 Troubleshooting](../../wiki/Troubleshooting)** | 401s, premium gating, validation errors, writes that change nothing |
| **[🔀 Migration](../../wiki/Migration)** | Upgrading from rusteero 1.0.0 |
| **[🏠 Wiki Home](../../wiki)** | All documentation |
| **[✅ Parity checklist](PARITY.md)** | Method-by-method status against `eero-api` |

## 🔗 Ecosystem

| Project | Description |
|---------|-------------|
| **[🐍 eero-api](https://github.com/fulviofreitas/eero-api)** | The Python library this crate is a port of |
| **[🖥️ eeroctl](https://github.com/fulviofreitas/eeroctl)** | Terminal interface for Eero networks (Python) |
| **[🛜 eero-ui](https://github.com/fulviofreitas/eero-ui)** | Svelte dashboard for network management |
| **[📊 eero-prometheus-exporter](https://github.com/fulviofreitas/eero-prometheus-exporter)** | Prometheus metrics for monitoring |

## 🙏 Acknowledgments

`rusteero` is a Rust client library for the Eero (Amazon) mesh Wi-Fi cloud API, ported from
[eero-api](https://github.com/fulviofreitas/eero-api). No code was copied from it or from any
other project; full lineage and licence attribution is in [NOTICE](NOTICE), and the long-form
version with what changed in the port is on the
[wiki](https://github.com/fulviofreitas/rusteero/wiki).

## ⚠️ Important Notes

> **Unofficial Project**: This library uses reverse-engineered APIs and is not affiliated with or endorsed by Eero or Amazon.

> **Amazon Login Limitation**: If your Eero account uses Amazon for login, this library may not work directly due to API limitations. **Workaround**: Have someone in your household create a standard Eero account (with email/password) and invite them as an admin to your network. Then use those credentials to authenticate.

## 📄 License

[MIT](LICENSE) — Use it, fork it, build cool stuff 🎉
