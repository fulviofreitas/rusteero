# ⚙️ Configuration

## Authentication Storage

`rusteero` persists a [`Session`](Rust-API#authentication) through any type implementing the
`CredentialStore` trait (`load`, `save`, `clear` — synchronous by design, since the OS keyring
API is itself blocking; async callers reach it through `spawn_blocking`-backed adapters
internally). Four backends ship in the crate.

### Keyring (feature `keyring`, on by default)

```rust
use rusteero::storage::KeyringStore;
let store = KeyringStore::new();
```

Stores the session in the OS-native credential store:

*   **macOS:** Keychain
*   **Linux:** Secret Service (GNOME Keyring, KWallet)
*   **Windows:** Credential Manager

The entry is **deliberately the same one the Python `eero-api` library uses** — service
`eero-api`, account `auth-tokens`, one JSON blob — so a login made with `eeroctl` is picked up by
`rusteero` and vice versa. This is a conscious design decision (not an accident of matching
constants), so a session created by either tool works unchanged in the other.

### File-based storage

```rust
use rusteero::storage::FileStore;
let store = FileStore::new("/home/me/.config/eero/cookies.json");   // same JSON format as eero-api
```

The library has no default path — you always choose one. The file is created atomically with
owner-only `0600` permissions (never `chmod`ed after the fact, so there is no world-readable
window while the write is in flight).

### Chained (primary + fallback)

```rust
use rusteero::storage::{ChainedStore, FileStore, KeyringStore};
use std::sync::Arc;

let store = ChainedStore::new(
    Arc::new(KeyringStore::new()),
    Arc::new(FileStore::new("/home/me/.config/eero/cookies.json")),
);
```

Reads and writes go through the primary first, falling back to the secondary backend. Unlike the
Python library — where the keyring backend can never itself report a save failure — a failed
primary save here genuinely triggers the fallback and clears the stale primary entry.

### Memory / your own backend

```rust
use rusteero::storage::MemoryStore;
let store = MemoryStore::new();   // in-process only; nothing survives a restart
```

Implement `CredentialStore` yourself for anything else (Vault, a Kubernetes secret, 1Password,
...) — the trait only needs `load`, `save` and `clear`, each returning `Result<_, StorageError>`.

### `create_storage`: the four-way matrix

For the common cases, `create_storage` picks a backend for you from a `StorageConfig`:

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

`StorageConfig::default()` (and `::new()`) leaves both fields off — `false`/`None` — which yields
a bare `MemoryStore`. This is the opposite default from the Python library's own
`use_keyring=True`: a Rust library should not silently reach for the OS keyring unless a caller
explicitly opts in.

Under `--no-default-features` (no `keyring` feature), `use_keyring` is ignored and the matrix
degrades to the nearest non-keyring backend: the `true`+`Some` row becomes a bare `FileStore`,
and `true`+`None` becomes a bare `MemoryStore`.

## Builder options

```rust
use rusteero::auth::Session;
use rusteero::transport::StorageFailures;
use rusteero::Client;
use std::sync::Arc;
use std::time::Duration;

let client = Client::builder()
    .session(Some(Session::from_token("s-example")))   // Option<Session> — omit/None to load from `.store(..)`
    .store(Some(store))                                  // Option<Arc<dyn CredentialStore>>
    .storage_failures(StorageFailures::Warn)             // Warn (default) or Fatal
    .cache_ttl(Duration::from_secs(60))
    .user_agent(Some("my-tool/1.0".to_owned()))          // default: reqwest's own User-Agent
    .build()
    .await?;
```

If neither `.session(..)` nor `.store(..)` is set, the resulting `Client` starts with no session
at all and no persistence — there is no implicit default store. Set `.store(..)` explicitly (even
just `Some(Arc::new(MemoryStore::new()))`) if you want `set_session_token`/`clear_session_token`/
`logout` to persist anything.

### `StorageFailures`

Every Python `eero-api` storage backend swallows a save/load/clear failure internally and logs at
`DEBUG`. `rusteero` surfaces it as a typed `StorageError` instead, and lets you choose what
happens next:

*   `StorageFailures::Warn` (default) — log at `WARN`, proceed as if the operation succeeded. The
    in-memory session is always updated regardless of this setting; only *persistence* is
    affected.
*   `StorageFailures::Fatal` — return `Error::Storage` immediately instead.

### Disabling the keyring for headless use

The `keyring` feature is on by default. For a headless server, a container, or anywhere the OS
keyring isn't available, disable it:

```toml
[dependencies]
rusteero = { git = "https://github.com/fulviofreitas/rusteero", default-features = false }
```

`FileStore`, `MemoryStore`, `ChainedStore` and `create_storage` all still work; only
`KeyringStore` (and the keyring-preferring rows of `create_storage`'s matrix) are unavailable.

## Stored format

```json
{
  "session_id": "…",
  "refresh_token": null,
  "session_expiry": "2026-10-10T12:34:56"
}
```

`session_expiry` is a naive (timezone-less) local ISO 8601 string with second precision —
identical to what `eero-api` writes, so the file or keyring blob round-trips between the two
libraries. The legacy key `user_token` is accepted as a fallback for `session_id` on read, for
compatibility with older `eero-api` versions.

## Environment Variables

The library reads **no** environment variables on its own — there is no `EERO_CONFIG_DIR` or
similar implicit lookup. For CI/headless use, opt in explicitly:

```rust
use rusteero::auth::Session;
let session = Session::from_env("RUSTEERO_SESSION_TOKEN")?;
```

This reads the named variable, fabricates the same 30-day expiry `Session::from_token` would, and
returns `Error::Validation` if the variable is unset or empty.

## Security Properties

*   Session tokens are `secrecy::SecretString`; they never appear in `Debug` output, logs or
    error messages. `Envelope`'s own `Debug` impl prints only `meta.code` and the *shape* of
    `data` (object/array/etc.), never its contents, since a cached response can carry a Wi-Fi
    password.
*   The token is sent only as the `s=` cookie; HTTP redirects are refused
    (`reqwest::redirect::Policy::none()`) so it can never travel to another host on a `3xx`.
*   Response bodies are capped at 10 MiB (`consts::MAX_RESPONSE_BYTES`); error bodies embedded in
    an `Error::Api` are redacted, then truncated to 512 characters
    (`consts::MAX_ERROR_BODY_CHARS`).
*   `FileStore` writes are atomic and `0600` from the moment the file exists — never `chmod`ed
    after the fact.
*   Path segments built into a URL (network/device/profile ids) are validated before the request
    is sent, so an id carrying `..`, a stray `/` or a newline cannot escape its segment and
    redirect a write at the wrong resource.

---

## 🔗 Related Pages

*   [📖 Rust API](Rust-API)
*   [🔧 Troubleshooting](Troubleshooting)
*   [🏠 Home](Home)
