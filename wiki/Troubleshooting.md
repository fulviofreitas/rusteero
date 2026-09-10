# 🔧 Troubleshooting

## Authentication Issues

### Amazon Login Accounts

If your Eero account uses Amazon for login (Sign in with Amazon), this library may not work
directly due to API limitations.

**Workaround:**

1.  Have someone in your household create a standard Eero account using email and password (not Amazon login)
2.  In the Eero app, invite that account as an admin to your network
3.  Use those new credentials to authenticate with this library

```rust
// Use the new email/password account, not your Amazon-linked account
let pending = LoginFlow::new(None).start("new-admin@example.com").await?;
let session = pending.verify(&code).await?;
```

> **Note**: The invited admin account will have full access to manage the network.

### Session Expired

`Error::Authentication` on a previously working client usually means the session expired
(the library assumes 30 days). Re-run the login flow, or inject a fresh token with
`Session::from_token`.

### Check Authentication Status

```rust
if !client.is_authenticated() { /* run LoginFlow */ }
```

### Keyring unavailable (headless Linux, containers)

`KeyringStore` returns `StorageError`; with the default `storage_failures(Warn)` the client
still works for the session and logs a warning. Use `FileStore` or `MemoryStore`, or build
with `default-features = false`.

## Network Connection Issues

### No Networks Found

`get_networks` falls back to `GET /account` and reads `data.networks` when the list is empty,
exactly like `eero-api`. If both are empty the account really has no networks.

### Device changes are ignored

Nickname/pause writes go to API version `2.3`; the `2.2` path returns 200 and silently drops
them. Blocking uses `POST /blacklist`. If you built a custom route, check `routes.rs`.

### API Timeouts

Defaults: 30 s total, 10 s read. Supply your own `reqwest::Client` via `.http(..)` to change them.

### Rate limited

`Error::RateLimit { retry_after }` — back off for `retry_after` if present. The library does not
retry automatically.

## Debug Logging

```rust
tracing_subscriber::fmt().with_env_filter("rusteero=debug").init();
```

Logs contain method, path and status only — never tokens, cookies or bodies.

---

## 🔗 Related Pages

*   [📖 Rust API](Rust-API)
*   [⚙️ Configuration](Configuration)
*   [🏠 Home](Home)
