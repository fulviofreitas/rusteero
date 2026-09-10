# ⚙️ Configuration

> 🚧 **Phase 0.** Describes the planned behaviour; updated as phases land.

## Authentication Storage

### Keyring (Default, feature `keyring`)

By default, credentials are stored securely in your system keyring:

*   **macOS:** Keychain
*   **Linux:** Secret Service (GNOME Keyring, KWallet) or kernel keyutils
*   **Windows:** Credential Manager

The entry is **the same one the Python `eero-api` library uses** (service `eero-api`, account
`auth-tokens`, one JSON blob), so a login made with `eeroctl` is picked up by `rusteero` and
vice-versa.

### File-based Storage

For systems without keyring support or headless environments, use a JSON file. The library has
no default path — you choose it:

```rust
use rusteero::storage::FileStore;
let store = FileStore::new("~/.config/eero/cookies.json");   // same format as eero-api
```

The file is created with `0600` permissions. Expired sessions are cleared on load.

### Chained (keyring with file fallback)

```rust
use rusteero::storage::{ChainedStore, KeyringStore, FileStore};
let store = ChainedStore::new(KeyringStore::new(), FileStore::new("…/cookies.json"));
```

Unlike the Python library, a keyring failure really does fall through to the file.

### Memory / custom

`MemoryStore` keeps the session in process memory only. Implement the `CredentialStore` trait
(`load`, `save`, `clear`) for anything else (Vault, Kubernetes secret, 1Password …).

## Builder options

```rust
let client = rusteero::Client::builder()
    .session(session)                  // pre-obtained Session (optional)
    .store(store)                      // CredentialStore (default: MemoryStore)
    .storage_failures(Warn)            // Warn (default) or Fatal
    .cache_ttl(Duration::from_secs(60))
    .user_agent("my-tool/1.0")         // default: reqwest default
    .build()?;
```

## Stored format

```json
{
  "session_id": "…",
  "refresh_token": null,
  "session_expiry": "2026-10-10T12:34:56"
}
```

`session_expiry` is local, timezone-naive ISO 8601 — identical to what `eero-api` writes. The
legacy key `user_token` is accepted on read.

## Environment Variables

The library reads **no** environment variables on its own. For CI/headless use, opt in
explicitly:

```rust
let session = rusteero::Session::from_env("RUSTEERO_SESSION_TOKEN")?;
```

## Security Considerations

*   Session tokens are `secrecy::SecretString`; they never appear in `Debug` output, logs or error messages.
*   The token is sent only as the `s=` cookie; HTTP redirects are refused so it can never travel to another host.
*   Response bodies are capped at 10 MiB; error bodies are truncated to 512 characters.
*   Disable the keyring feature for headless builds: `default-features = false`.

---

## 🔗 Related Pages

*   [📖 Rust API](Rust-API)
*   [🔧 Troubleshooting](Troubleshooting)
*   [🏠 Home](Home)
