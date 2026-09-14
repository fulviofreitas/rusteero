<div align="center">

# 🦀 rusteero

**Async Rust client for the Eero mesh Wi-Fi cloud API**

[![Rust](https://img.shields.io/badge/rust-1.96%2B-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-22c55e?style=for-the-badge)](LICENSE)
[![Status](https://img.shields.io/badge/status-pre--release%20%E2%80%94%20not%20published-f59e0b?style=for-the-badge)](#-status)

---

_`rusteero` is a Rust client library for the Eero (Amazon) mesh Wi-Fi cloud API._
_It is a port of the Python [`eero-api`](https://github.com/fulviofreitas/eero-api) library: same raw JSON contract, same method names, compatible credential storage._

[Status](#-status) · [Quick Start](#-quick-start) · [Docs](#-docs) · [Acknowledgments](#-acknowledgments) · [License](#-license)

</div>

---

## ⚡ Why rusteero?

- 🚀 **Async-first** — `tokio` + `reqwest`, no Python runtime
- 📦 **Raw JSON** — every call returns the exact `{meta, data}` envelope Eero sends; typed models are opt-in
- 🔐 **Secure** — session tokens are `SecretString`s, redirects are refused, system keyring optional
- 🔁 **Compatible** — a session created by [`eeroctl`](https://github.com/fulviofreitas/eeroctl) or `eero-api` works unchanged
- 🧩 **Headless-friendly** — the email/SMS code step is separable; inject a pre-obtained token instead

## 🚧 Status

**Feature-complete, not yet released.** Every method in the Python library is ported: transport,
authentication, credential storage, all 25 endpoint modules, the client facade and its cache, and
every mutating call. [`PARITY.md`](PARITY.md) tracks all 127 rows — none are outstanding — and
records a read-only sweep against a real account.

Nothing is published to crates.io until the library is validated against a live account and the
author signs off.

## 📦 Install

Not on crates.io yet — use the git dependency:

```toml
[dependencies]
rusteero = { git = "https://github.com/fulviofreitas/rusteero" }

# Headless / embedded (no system keyring):
rusteero = { git = "https://github.com/fulviofreitas/rusteero", default-features = false }
```

## 🚀 Quick Start

```rust
use rusteero::Client;
use rusteero::auth::flow::LoginFlow;

#[tokio::main]
async fn main() -> Result<(), rusteero::Error> {
    // Interactive step (once): email or phone → one-time code → session
    let pending = LoginFlow::new(None)?.start("you@example.com").await?;
    let code = rpassword::prompt_password("Code: ").unwrap();
    let session = pending.verify(code.trim()).await?;

    // Or, headless: let session = rusteero::Session::from_env("RUSTEERO_SESSION_TOKEN")?;

    let client = Client::builder().session(Some(session)).build().await?;

    // Every method returns the raw JSON envelope, exactly as Eero sent it
    let networks = client.get_networks(false).await?;
    for n in networks.data()["networks"].as_array().into_iter().flatten() {
        println!("📶 {}: {}", n["name"], n["status"]);
    }
    Ok(())
}
```

> 💡 A `Client` persists nothing unless you give it a credential store. Attach one — the system
> keyring is available under the default `keyring` feature — and it uses the *same* entry the
> Python library does, so `eeroctl` and `rusteero` share one login. See
> [Configuration](https://github.com/fulviofreitas/rusteero/wiki/Configuration).

## 📄 Raw Response Format

All API methods return the exact JSON from Eero's API, wrapped in a lossless `Envelope`:

```json
{
  "meta": { "code": 200, "server_time": "..." },
  "data": { "...": "endpoint-specific payload" }
}
```

`env.meta()`, `env.data()`, `env.into_value()` and `env.data_as::<T>()` are the only helpers.

## 📚 Docs

| Guide | What's inside |
|-------|---------------|
| **[📖 Rust API](../../wiki/Rust-API)** | Full API reference |
| **[⚙️ Configuration](../../wiki/Configuration)** | Auth storage & builder options |
| **[🔧 Troubleshooting](../../wiki/Troubleshooting)** | Common fixes |
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
