# ⚙️ Configuration

## Sessions

A `Session` (`rusteero::Session`) is the long-lived token sent as the `X-User-Token` header —
and, unless disabled, as a legacy `Cookie: s=<token>` — on every authenticated request. It is
**only** a token: there is no client-side expiry and no refresh token. The server is the sole
authority on whether it still works, and says so with a 401.

| Source | Call | Validation |
|---|---|---|
| Interactive login | `LoginFlow::new(None)?.start(email_or_phone).await?` → `PendingLogin::verify(code).await?` | Server-side; the login token becomes the session token |
| Environment variable | `Session::from_env("RUSTEERO_SESSION_TOKEN")?` | `Error::Validation` when unset, empty, or not printable ASCII |
| A string you hold | `Session::from_token("<token>")` | None — validated later by `Transport::set_session`/`AuthApi::set_session_token` |
| A credential store | `Client::builder().store(Some(store)).build().await?` | Loaded once at `build()` when no explicit `.session(..)` was given |
| Existing client | `client.set_session_token("<token>")?` | Non-empty, printable ASCII, no CR/LF; persisted through the store; clears the cache |

`client.session()` returns a snapshot as `Option<Session>`; the token stays a
`secrecy::SecretString` and `Session`'s `Debug`/`Display` never print it.

The library reads **no** environment variables on its own — `Session::from_env` is an explicit
opt-in with a name you choose.

## Credential stores

`rusteero` persists a `Session` through any type implementing `CredentialStore`
(`rusteero::storage::CredentialStore`): three synchronous methods, `load`, `save`, `clear`, each
returning `Result<_, StorageError>`. The trait is synchronous on purpose — every OS keyring API
blocks — and async callers reach it through `spawn_blocking` adapters internally. `load` returns
`Session::empty()`, not an error, when nothing has been stored.

### `MemoryStore`

```rust
use rusteero::storage::MemoryStore;
let store = MemoryStore::new();   // in-process only; nothing survives a restart
```

### `FileStore`

```rust
use rusteero::storage::FileStore;
let store = FileStore::new("/home/me/.config/eero/cookies.json");   // the eero-api cookie file
```

No default path — you always choose one, and `~`/relative paths are not expanded. On Unix the
file is written to a temporary sibling created atomically with owner-only `0600` permissions,
`fsync`ed, then renamed into place: never a `chmod` after the fact, never a partially written
target, and a symlink planted at the path is replaced rather than followed. On non-Unix
platforms the OS default ACL applies; treat the containing directory as the access boundary.

### `KeyringStore` (feature `keyring`, on by default)

```rust
use rusteero::storage::KeyringStore;
let store = KeyringStore::new();                                  // the shared eero-api entry
let store = KeyringStore::with_entry("my-service", "my-account");  // an isolated entry
```

Backed by the OS-native store — macOS Keychain, Secret Service (GNOME Keyring, KWallet) on
Linux, Windows Credential Manager. `KeyringStore::new()` uses **the same service/account entry
the Python `eero-api` library uses**, so a login made with `eeroctl` is picked up by `rusteero`
and vice versa; `with_entry` is for tests or several accounts on one machine.

### `ChainedStore`

```rust
use rusteero::storage::{ChainedStore, FileStore, KeyringStore};
use std::sync::Arc;

let store = ChainedStore::new(
    Arc::new(KeyringStore::new()),
    Arc::new(FileStore::new("/home/me/.config/eero/cookies.json")),
);
```

`load` reads the primary and, when it holds no usable session **or fails outright** (a keyring
daemon that is not running), falls through to the fallback, migrating a hit back into the
primary on a best-effort basis. `save` writes to the primary; only if that fails does it clear
the stale primary entry (best-effort) and write to the fallback — unlike the Python library,
where the keyring backend never reports a failure and the fallback branch is dead. `clear`
clears both and reports which backend still holds the credential if one of them failed
(`StorageError::Backend` with `backend: "chained-primary"` / `"chained-fallback"`, or
`"chained"` when both failed).

### `create_storage`: the four-way matrix

```rust
use rusteero::storage::{create_storage, StorageConfig};

let store = create_storage(&StorageConfig {
    use_keyring: true,
    cookie_file: Some("/home/me/.config/eero/cookies.json".into()),
});
```

| `use_keyring` | `cookie_file` | Result |
|---|---|---|
| `true` | `Some(path)` | `ChainedStore` — `KeyringStore` primary, `FileStore` fallback |
| `true` | `None` | bare `KeyringStore` |
| `false` | `Some(path)` | bare `FileStore` |
| `false` | `None` | bare `MemoryStore` |

`StorageConfig::default()` (and `::new()`) leaves both fields off, yielding a `MemoryStore` —
the opposite of the Python library's `use_keyring=True`: a Rust library should not reach for
the OS keyring unless asked. Under `--no-default-features` the keyring rows degrade to a bare
`FileStore` / `MemoryStore`.

### `StorageFailures`

Every Python `eero-api` backend swallows a save/load/clear failure and logs at `DEBUG`.
`rusteero` surfaces it as a typed `StorageError` and lets you choose what happens next:

*   `StorageFailures::Warn` (default) — log at `WARN`, proceed as if the operation succeeded.
    The in-memory session is always updated first, regardless of this setting; only
    *persistence* is affected.
*   `StorageFailures::Fatal` — return `Error::Storage` instead. This also applies to the initial
    load in `ClientBuilder::build()` and to the store clear inside `logout()`.

`StorageError` variants: `Io`, `Serde`, `Backend { backend, message }`, `NotFound`, `Empty`.
`message` never contains the credential value.

## The stored record (schema 2)

```json
{"session_id": "…", "schema_version": 2}
```

This is the exact record `eero-api` 8.x writes to its cookie file and keyring entry, so it
round-trips between the two libraries. An empty session is written as `"session_id": null`.

**Legacy migration.** A record with no `schema_version` key — one written by `eero-api` 7.x or
earlier, or by `rusteero` 1.0.0, possibly carrying `refresh_token`, `session_expiry`, or the
pre-3.0 `user_token` key — is migrated the first time it is loaded: the token is read from
`session_id`, falling back to `user_token`; every other field is dropped; the record is
re-saved in the shape above and read back. The read-back logs `debug!` on a match or `warn!`
on a mismatch but **never fails the load** — the in-memory session is returned either way. No
re-authentication is needed.

If something outside the library writes the file, write the shape above including
`schema_version: 2`; a record without the marker is treated as legacy and rewritten.

## Builder options

```rust
use rusteero::transport::StorageFailures;
use rusteero::{Client, Session};
use std::sync::Arc;
use std::time::Duration;

let client = Client::builder()
    .session(Some(Session::from_token("<token>")))   // Option<Session>; omit to load from `.store(..)`
    .store(Some(store))                              // Option<Arc<dyn CredentialStore>>
    .storage_failures(StorageFailures::Warn)         // Warn (default) or Fatal
    .cache_ttl(Duration::from_secs(60))              // Duration::ZERO disables cache reads
    .user_agent(Some("my-tool/1.0".to_owned()))      // default "eero/3.0 (iPhone; iOS 17.0)"
    .accept_language("en-US")                        // X-Accept-Language; default "en-US"
    .send_legacy_cookie(true)                        // default true
    .get_retries(0)                                  // default 0
    .build()
    .await?;
```

If neither `.session(..)` nor `.store(..)` is set, the `Client` starts with no session and no
persistence — there is no implicit default store. If both are set, the explicit session wins
and the store is only used for later `set_session_token`/`clear_session_token`/`logout`
persistence.

### Transport options

| Option | What it does |
|---|---|
| `send_legacy_cookie(bool)` | `true` (default) sends `Cookie: s=<token>` alongside the primary `X-User-Token` header; `false` sends the header only. Both are attached only when the request URL's scheme, host and port match the configured API host — a request anywhere else goes out without a credential and logs a `WARN` |
| `accept_language(..)` | The `X-Accept-Language` header on every request. Printable ASCII with no CR/LF, checked at `build()` |
| `user_agent(Option<String>)` | The `User-Agent` header; `None` sends the crate default. Same validation |
| `get_retries(u32)` | Additional attempts for a `GET` that fails with `Error::Network`, `Error::Timeout` or a `5xx`, 500 ms apart, each logged at `WARN`. Writes are never retried; `4xx` and `429` are never retried; the one-shot 401 refresh-and-replay is separate |
| `base_url(..)` | Replaces the API host for **both** `/2.2` and `/2.3`; also what link resolution and the credential gate compare against. Meant for a local mock server in tests |
| `http(reqwest::Client)` | Your own HTTP client. Discards the crate's redirect refusal and its timeouts; rebuild both (`.redirect(reqwest::redirect::Policy::none())`, `.timeout(..)`, `.read_timeout(..)`) if you need them |

Timeouts are 30 s per request and 10 s per read (`consts::REQUEST_TIMEOUT`,
`consts::READ_TIMEOUT`). `ClientBuilder` has no timeout setters; `Transport::builder()` exposes
`timeout(..)` and `read_timeout(..)` for a caller building an `EeroApi` by hand, and both are
ignored when `http(..)` is supplied. Every request also carries `Accept: application/json`;
`Content-Type` is set per request by the body encoding (JSON, form, or the literal `""`).

Response bodies are capped at 10 MiB (`consts::MAX_RESPONSE_BYTES`); a `3xx` is refused before
the body is read; an empty or `204` body becomes an empty `Envelope`.

### Disabling the keyring for headless use

```toml
[dependencies]
rusteero = { git = "https://github.com/fulviofreitas/rusteero", default-features = false }
```

`FileStore`, `MemoryStore`, `ChainedStore` and `create_storage` all still work; only
`KeyringStore` (and the keyring rows of the matrix) are unavailable.

## Logging and redaction

The crate logs through `tracing`:

```rust
tracing_subscriber::fmt()
    .with_env_filter("rusteero=debug")
    .init();
```

*   `DEBUG`: method, rendered path and status of every request; for an error response, the
    parsed envelope **after** `rusteero::redact::redact_sensitive` has replaced sensitive keys
    (tokens, passwords, identifier-shaped values). Never headers, never cookies, never raw body
    text.
*   `WARN`: a GET retry, a credential withheld from an off-host request, a store failure under
    `StorageFailures::Warn`, a failed `logout` request, a legacy-record migration mismatch, and
    one fixed line before every uncharacterised write (see
    [Rust API — Writes and safety](Rust-API#writes-and-safety)). None of these lines carries a
    caller-supplied identifier or a credential.

Error messages are fixed labels; `Error::envelope()` holds the raw response if you need it. If
you log an envelope yourself, pass it through `redact_sensitive` first — a network envelope
carries the Wi-Fi password.

## Security properties

*   Session tokens are `secrecy::SecretString`; they never appear in `Debug` output, logs or
    error messages. `Envelope`'s `Debug` prints only `meta.code` and the *shape* of `data`.
*   The credential goes only to the configured API host (scheme, host and port compared), and
    redirects are refused (`reqwest::redirect::Policy::none()`), so a `3xx` can never carry the
    token elsewhere. A caller-supplied client that followed a redirect is still detected and
    refused before the body is read.
*   A bare id must be a single path segment (`[A-Za-z0-9][A-Za-z0-9._:-]*`, no `..`); a link
    value read from an envelope must be host-relative; an absolute URL must be on the API host.
    All of this is checked before a request is built.
*   `FileStore` writes are atomic and `0600` from the moment the file exists (Unix).
*   Every write that has not been characterised against a live network logs a warning first.

---

## 🔗 Related Pages

*   [📖 Rust API](Rust-API)
*   [🔧 Troubleshooting](Troubleshooting)
*   [🔀 Migration](Migration)
*   [🏠 Home](Home)
