# 🦀 rusteero Wiki

Welcome! Everything you need to use `rusteero`, the Rust client for the Eero mesh Wi-Fi cloud API.

## 📚 Guides

| Page | What you'll learn |
|------|-------------------|
| **[📖 Rust API](Rust-API)** | Full API reference & examples |
| **[⚙️ Configuration](Configuration)** | Auth storage & builder options |
| **[🔧 Troubleshooting](Troubleshooting)** | Common issues & fixes |

---

## 🚀 Quick Start

### Install

> 🚧 Not published to crates.io yet. Until then, depend on the git repository.

```toml
[dependencies]
rusteero = { git = "https://github.com/fulviofreitas/rusteero" }
```

<details>
<summary>📦 Build from source</summary>

```bash
git clone https://github.com/fulviofreitas/rusteero.git
cd rusteero
cargo build
cargo test --all-features
```

</details>

### Hello World

```rust
use rusteero::{Client, auth::LoginFlow};

#[tokio::main]
async fn main() -> Result<(), rusteero::Error> {
    let pending = LoginFlow::new(None).start("you@example.com").await?;
    let code = rpassword::prompt_password("Code: ").unwrap();
    let session = pending.verify(code.trim()).await?;

    let client = Client::builder().session(session).build()?;
    let networks = client.get_networks(false).await?;   // raw {meta, data} envelope
    for n in networks.data()["networks"].as_array().into_iter().flatten() {
        println!("📶 {}: {}", n["name"], n["status"]);
    }
    Ok(())
}
```

---

## 🔗 Links

| Resource | URL |
|----------|-----|
| 📦 crates.io | _not published yet_ |
| 📦 GitHub | [fulviofreitas/rusteero](https://github.com/fulviofreitas/rusteero) |
| 🐛 Issues | [Report a bug](https://github.com/fulviofreitas/rusteero/issues) |
| 📋 Changelog | [CHANGELOG.md](https://github.com/fulviofreitas/rusteero/blob/master/CHANGELOG.md) |
| ✅ Parity | [PARITY.md](https://github.com/fulviofreitas/rusteero/blob/master/PARITY.md) |
| 🐍 Python original | [eero-api](https://github.com/fulviofreitas/eero-api) |

---

## 🙏 Acknowledgments

`rusteero` is a Rust port of [eero-api](https://github.com/fulviofreitas/eero-api) by
[@fulviofreitas](https://github.com/fulviofreitas), which is itself a modern revamp of the
original [eero-client](https://github.com/343max/eero-client) by
[@343max](https://github.com/343max). The Rust login/verify handshake was cross-checked
against [eero-rs](https://github.com/ssnover/eero-rs) by [@ssnover](https://github.com/ssnover),
an earlier Rust adaptation of the same original client. **No code was copied from any of
them** — the designs, endpoint tables and behaviours are derived from `eero-api`, whose MIT
notice is reproduced in the repository's
[NOTICE](https://github.com/fulviofreitas/rusteero/blob/master/NOTICE) file. `eero-rs` carries
no licence and was consulted as a reference only.

**What's new** (relative to `eero-api`):

*   Native async Rust on `tokio` + `reqwest` (rustls), no Python runtime
*   Same raw `{meta, data}` JSON contract, plus an `Envelope` helper and opt-in typed models
*   Interactive email/SMS code flow separated from the client (`LoginFlow` → `Session`), so
    headless and CI consumers can inject a pre-obtained session token
*   Pluggable `CredentialStore` trait: system keyring (optional feature), file, memory, or your own
*   Credential storage format is wire-compatible with `eero-api`, so a session created by
    `eeroctl` works unchanged
*   Session tokens are `SecretString`s and never appear in logs
*   Keyring failures fall back to file storage (the Python fallback never fired)
*   Rate-limit errors carry `Retry-After` when the server sends it
*   Every wire endpoint is a single `Route` constant, so upstream API drift is a one-line fix
*   Removed-upstream endpoints (`activity/*`, device priority) are not carried over
*   Response bodies embedded in errors are redacted before truncation, so a malformed reply
    cannot leak a session token or a Wi-Fi password into a log line
*   Path segments are validated before a URL is built, so an id carrying a stray newline
    cannot escape its segment and send a write at the wrong resource

---

## ⚠️ Important Notes

> **Unofficial Project**: This library uses reverse-engineered APIs and is not affiliated with or endorsed by Eero or Amazon.

> **Amazon Login Limitation**: If your Eero account uses Amazon for login, this library may not work directly due to API limitations. See [Troubleshooting](Troubleshooting#amazon-login-accounts) for the workaround.

---

## 💡 Tips

*   Use the sidebar to navigate between pages
*   Code blocks have a copy button
*   Each page has a table of contents
