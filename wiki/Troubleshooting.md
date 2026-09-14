# 🔧 Troubleshooting

## Authentication Issues

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

### Session Expired

`Error::Authentication` on a previously working client usually means the session's
client-fabricated 30-day expiry has passed. There is no server-driven refresh in practice — the
Eero cloud API has never been observed issuing a refresh token at login — so the fix is simply to
run the login flow again, or inject a fresh token:

```rust
use rusteero::auth::Session;
let session = Session::from_token("s-a-newly-obtained-token");
```

### Check Authentication Status

```rust
if !client.is_authenticated() {
    // run LoginFlow again, or inject a fresh Session
}
```

`is_authenticated()` is a purely local check (token present, not past its expiry) — it never
makes a network call.

### `Error::MissingNetworkId`

Returned when a method needs a network id, none was passed explicitly, no preferred network was
set via `client.set_preferred_network(..)`, and (for methods that attempt it) auto-discovery
found no networks either. Either pass an explicit `network_id`, call
`client.set_preferred_network("...")` once after login, or check that `get_networks()` actually
returns something for this account.

### Keyring unavailable (headless Linux, containers)

`KeyringStore` returns a `StorageError` when there's no Secret Service (or equivalent) running.
With the default `storage_failures(StorageFailures::Warn)`, the in-memory session still works for
the lifetime of the process — only persistence fails, and it's logged at `WARN`, not returned as
an error. For headless environments, prefer `FileStore` or `MemoryStore` explicitly, or build
with `default-features = false` to drop the `keyring` dependency entirely (see
[Configuration](Configuration#disabling-the-keyring-for-headless-use)).

## Network / API Issues

### No Networks Found

`get_networks()` falls back to `GET /account` and reads `data.networks` when the direct
`/networks` list comes back empty — this has been observed on a real account where `/networks`
itself returned nothing and the `/account` fallback is what actually worked. If both come back
empty, the account genuinely has no networks associated with it.

### Some endpoints 404 (or 403) even though the crate builds the right URL

A handful of read endpoints have been observed returning 404/403 on a real, working account:
`get_settings` (404), `get_password` (404), `get_transfer_stats` (403 — looks tier/permission
gated), `get_backup_network` (404 — no backup internet configured), `get_backup_status` (404),
`get_ouicheck` (404), `get_burst_reporters` (404). These are almost certainly account/feature
gating on Eero's side, not a bug in this crate or a wrong route — the corresponding `PUT`s (for
the settings that have one) work normally. If you hit one of these, it is expected; there is
nothing to configure differently on the client side. See
[PARITY.md](https://github.com/fulviofreitas/rusteero/blob/master/PARITY.md)'s "Live validation"
section for the full list and how it was confirmed.

### Device changes are ignored

Nickname and pause writes must go to API version `2.3` — the `2.2` path accepts the same `PUT`
and returns `200 OK`, but the write is silently dropped server-side. This crate already routes
every device mutation to `2.3`; if you're seeing a write that doesn't stick, check
`src/routes.rs` for the actual route in use rather than assuming the version. Blocking a device
uses `POST /blacklist` (plus a `GET` to resolve its MAC) and unblocking uses `DELETE
/blacklist/{id}` — a `PUT {"blocked": …}` is a no-op on the real API, not just in this crate.

### API Timeouts

Defaults: 30 s total, 10 s per read (`consts::REQUEST_TIMEOUT` / `consts::READ_TIMEOUT`). Supply
your own `reqwest::Client` via `.http(..)` on `ClientBuilder`/`TransportBuilder` to change them —
but see that method's own warning: a caller-supplied client also discards this crate's redirect
refusal, so rebuild both settings yourself (`.redirect(reqwest::redirect::Policy::none())` plus
your own `.timeout(..)`/`.read_timeout(..)`) rather than dropping just one.

### Rate limited

`Error::RateLimit { retry_after }` — back off for `retry_after` if the server sent a
`Retry-After` header (`retry_after` is `None` otherwise). The library never retries a rate-limited
request automatically.

## Debug Logging

```rust
tracing_subscriber::fmt()
    .with_env_filter("rusteero=debug")
    .init();
```

Logs contain method, path and status only. Headers, cookies and response bodies are never
logged at any level, and any response body embedded in an `Error::Api` message is redacted
before it's truncated — a malformed reply can't leak a session token or a Wi-Fi password into a
log line this way.

---

## 🔗 Related Pages

*   [📖 Rust API](Rust-API)
*   [⚙️ Configuration](Configuration)
*   [🏠 Home](Home)
