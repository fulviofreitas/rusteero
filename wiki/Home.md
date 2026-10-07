# 🦀 rusteero Wiki

Welcome! Everything you need to use `rusteero`, the async Rust client for the Eero mesh Wi-Fi
cloud API — a port of the Python [`eero-api`](https://github.com/fulviofreitas/eero-api)
library at **v8.0.4**.

## 📚 Guides

| Page | What you'll learn |
|------|-------------------|
| **[📖 Rust API](Rust-API)** | The `Client` → `EeroApi` → `Transport` layers, builder options, every domain accessor, caching, network targeting, errors |
| **[⚙️ Configuration](Configuration)** | `Session` sources, credential stores and the schema-2 record, transport options, logging |
| **[🔧 Troubleshooting](Troubleshooting)** | 401s, premium gating, local validation errors, writes that return 200 and change nothing, keyring on headless Linux |
| **[🔀 Migration](Migration)** | Upgrading from rusteero 1.0.0 to 2.0.0 |

---

## 🚀 Quick Start

### Install

> 📦 Published on crates.io as `rusteero`; API docs on docs.rs.

```toml
[dependencies]
rusteero = "2"
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
use rusteero::auth::flow::LoginFlow;
use rusteero::Client;
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), rusteero::Error> {
    let pending = LoginFlow::new(None)?.start("you@example.com").await?;

    print!("Code: ");
    std::io::stdout().flush().ok();
    let mut code = String::new();
    std::io::stdin().read_line(&mut code).ok();
    let session = pending.verify(code.trim()).await?;

    // ClientBuilder::build is async: it may load a stored session from a configured
    // CredentialStore.
    let client = Client::builder().session(Some(session)).build().await?;

    let networks = client.get_networks(false).await?;   // raw {meta, data} envelope
    for n in networks.data()["networks"].as_array().into_iter().flatten() {
        println!("📶 {}: {}", n["name"], n["status"]);
    }
    Ok(())
}
```

---

## 🧭 How it is put together

```
Client            cache + network-id resolution + parent envelopes   (eero-api: EeroClient)
  └─ EeroApi      37 domain accessors over one shared transport      (eero-api: EeroAPI)
       └─ Transport   headers, credential placement, status → Error, refresh-and-replay
```

* Every endpoint returns `Result<Envelope, Error>` over the untouched `{"meta": …, "data": …}`
  wire payload.
* Method names are the Python names verbatim, in `snake_case`, so a method can be searched for by
  name across both libraries.
* Every resource argument accepts a bare id, an API path or an absolute API-host URL; every
  domain method takes a `parent` envelope so the request follows the link the API published.
* Errors are classified against the API's closed catalogue of `meta.error` strings; every error
  carries `error_code()` and `envelope()`.

---

## 🔗 Links

| Resource | URL |
|----------|-----|
| 📦 crates.io | <https://crates.io/crates/rusteero> |
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
*   Same raw `{meta, data}` JSON contract, plus an `Envelope` helper (`meta()`, `data()`,
    `data_as::<T>()`, `into_value()`)
*   Interactive email/SMS code flow separated from the client (`LoginFlow` → `PendingLogin` →
    `Session`), so headless and CI consumers can inject a pre-obtained session token
*   Pluggable `CredentialStore` trait: system keyring (optional feature), file, memory, chained,
    or your own; the persisted record is byte-compatible with `eero-api`, so a session created by
    `eeroctl` works unchanged
*   Session tokens are `SecretString`s and never appear in `Debug` output, logs or error messages
*   Storage failures surface as a typed `StorageError` with a `Warn`/`Fatal` policy, instead of
    being swallowed
*   `ChainedStore` genuinely falls back when the primary backend fails to save
*   Rate-limit errors carry `Retry-After` when the server sends it
*   Every wire endpoint is a route constant, so upstream API drift is a one-line fix
*   Path segments and link values are validated before a URL is built, and the credential is
    withheld from any request that would leave the configured API host

---

## ⚠️ Important Notes

> **Unofficial Project**: This library uses reverse-engineered APIs and is not affiliated with or endorsed by Eero or Amazon.

> **Amazon Login Limitation**: If your Eero account uses Amazon for login, this library may not work directly due to API limitations. See [Troubleshooting](Troubleshooting#amazon-login-accounts) for the workaround.

---

## 💡 Tips

*   Use the sidebar to navigate between pages
*   Code blocks have a copy button
*   Each page has a table of contents
