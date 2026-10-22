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
| A string you hold | `Session::from_token("<token>")` | None — validated when installed through `AuthApi::set_session_token` or `Transport::set_session` / `set_session_async` |
| A credential store | `Client::builder().store(Some(store)).build().await?` | Loaded once at `build()` when no explicit `.session(..)` was given |
| Existing client | `client.set_session_token("<token>").await?` | Non-empty, printable ASCII, no CR/LF (`field: "token"`); installed in memory at once, persisted through the store on Tokio's blocking pool; resets the cache and forgets an auto-discovered network |

`client.session()` returns a snapshot as `Option<Session>`; the token stays a
`secrecy::SecretString` and `Session`'s `Debug`/`Display` never print it.
`client.credentials_persisted()` says whether the most recent save reached the store and was
kept (see [`StorageFailures`](#storagefailures)).

The library reads **no** environment variables on its own — `Session::from_env` is an explicit
opt-in with a name you choose.

## Credential stores

`rusteero` persists a `Session` through any type implementing `CredentialStore`
(`rusteero::storage::CredentialStore`): three synchronous methods, `load`, `save`, `clear`, each
returning `Result<_, StorageError>`. The trait is synchronous on purpose — every OS keyring API
blocks. Async callers reach any `Arc<dyn CredentialStore>` through the public
`rusteero::storage::{load_async, save_async, clear_async}` adapters, which run the call on
Tokio's blocking pool (one hop per operation, read-back and migration included); the crate's own
session changes use them too. Dropping the returned future after its first poll does not cancel
the store call, and what the store logs on the blocking thread is recorded under the caller's
`tracing` dispatcher and span. `load` returns `Session::empty()`, not an error, when nothing has
been stored.

**Verified saves.** `FileStore`, `KeyringStore` and `ChainedStore` read the store back after
every `save` and return `Ok(())` only if the saved token is there. A write the backend accepted
but did not keep is `StorageError::NotRetained { backend }`; a write it rejected is the
backend's own error. `MemoryStore` counts as retained; a third-party store's `Ok(())` is trusted.

**Serialised operations.** `FileStore` and `KeyringStore` run each `load`, `save` and `clear`
under an internal lock, shared by every clone of the store, so a `save` and a `clear` on one
instance cannot interleave and call order decides the outcome. Separately constructed stores on
the same file or keyring entry are not coordinated with each other. A third-party store that
touches shared state should serialise its own operations, since async callers can have several
in flight on different blocking-pool threads.

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
After the rename the file is read back; if it is missing, unparseable or holds another token,
the store logs `Credential file did not retain the saved session` at `WARN` and returns
`StorageError::NotRetained`.

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

Every save is read back through a fresh entry. A keyring that accepts the write and keeps
nothing is `StorageError::NotRetained`, one that rejects it is `StorageError::Backend`; either
way the store logs one fixed `WARN` (`Keyring did not retain the session; credentials are not
persisted and will be lost when this process exits`), never the token. A clear that fails logs
`Keyring could not be cleared; a stored session may remain in it`. Silence both lines with
`KeyringStore::new().warn_on_unpersisted(false)` when a file fallback sits behind the keyring;
`create_storage` does so for its chained row, where the `ChainedStore` reports instead. The failed save is still reported as an error.

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
daemon that is not running), falls through to the fallback and promotes a hit back into the
primary on a best-effort basis; the fallback copy is cleared only once a read-back shows the
primary holds it. `save` writes to the primary and reads it back; only a *verified* primary
write skips the fallback. Otherwise it clears the primary's record (best effort, and only if it
is one this instance loaded or wrote; another process's record is left in place) and writes to
the fallback, which is read back too: `StorageError::Backend` (`backend: "chained"`) when
both writes failed, `StorageError::NotRetained` (`backend: "chained"`) when no backend was read
back holding the token. After a verified primary write the chain retires the fallback record,
but only one it knows is its own (the token it last loaded from, promoted out of, or wrote to
the fallback); a record another process left there is kept. `clear` clears both, logs a fixed
`WARN` if either failed, and reports which backend still holds the credential
(`StorageError::Backend` with `backend: "chained-primary"` / `"chained-fallback"`, or
`"chained"` when both failed) — a single failure is an error here, where `eero-api` only logs it.

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
| `true` | `Some(path)` | `ChainedStore` — `KeyringStore` primary (with `warn_on_unpersisted(false)`), `FileStore` fallback |
| `true` | `None` | bare `KeyringStore` |
| `false` | `Some(path)` | bare `FileStore` |
| `false` | `None` | bare `MemoryStore` |

`StorageConfig::default()` (and `::new()`) leaves both fields off, yielding a `MemoryStore` —
the opposite of the Python library's `use_keyring=True`: a Rust library should not reach for
the OS keyring unless asked. Under `--no-default-features` the keyring rows degrade to a bare
`FileStore` / `MemoryStore`.

### `StorageFailures`

Every Python `eero-api` backend swallows a save/load failure and a single backend's clear
failure (`ChainedStorage.clear` re-raises only when both backends fail): it logs it (at `DEBUG` to
`ERROR`, depending on the backend) and reports a failed save only through
`credentials_persisted`. `rusteero` surfaces it as a typed `StorageError` and lets you choose
what happens next:

*   `StorageFailures::Warn` (default) — log at `WARN`, proceed as if the operation succeeded.
    The in-memory session is always updated first, regardless of this setting; only
    *persistence* is affected. A `NotRetained` failure is logged by the store itself (the fixed
    texts above), so the transport records it at `DEBUG` only, never as a second `WARN`.
*   `StorageFailures::Fatal` — return `Error::Storage` instead. This also applies to the initial
    load in `ClientBuilder::build()` and to the store clear inside `logout()`.

Either way, `credentials_persisted()` (on `Client`, `EeroApi`, `AuthApi` and `Transport`)
reports the outcome of the most recent persist that reached the store: `true` initially, when no
store is configured, after a successful save, and after a clear (even one that failed, since the
credential being destroyed no longer needs to survive a restart); `false` after a failed or
not-retained save, or a save whose blocking task was cancelled before it ran. A persist skipped
because a newer session change overtook it, and a rejected token, leave it untouched. It never
affects whether the session works in memory.

`StorageError` variants: `Io`, `Serde`, `Backend { backend, message }`,
`NotRetained { backend }`, `NotFound`, `Empty`. No variant's text ever contains the credential
value. The enum is `#[non_exhaustive]`.

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
    .cache_ttl(Duration::from_secs(60))              // Duration::ZERO disables caching
    .user_agent(None)                                // None = "eero-ios/2026.8.1 (iPhone; iOS 18.5)"
    .accept_language("en-US")                        // X-Accept-Language; default "en-US"
    .send_legacy_cookie(true)                        // default true
    .get_retries(0)                                  // default 0
    .build()
    .await?;
```

If neither `.session(..)` nor `.store(..)` is set, the `Client` starts with no session and no
persistence — there is no implicit default store. If both are set, the explicit session wins
and the store is only used for later `set_session_token`/`clear_session_token`/
`clear_auth_data`/`logout` persistence, and the credential clear after a refresh that fails
terminally. `build()` validates `base_url`, `X-Accept-Language` and
`User-Agent` (in that order) before it reads the store, so a bad configuration fails before a
keyring prompt or a file read.

### The `User-Agent`

The default is `consts::DEFAULT_USER_AGENT`, `eero-ios/2026.8.1 (iPhone; iOS 18.5)`, the current
Eero iOS app's string. The API reads this header as the client's app version and decides which
capabilities to advertise from it: on the same account and network, the previous default
`eero/3.0 (iPhone; iOS 17.0)` (now `consts::LEGACY_USER_AGENT`) is offered markedly fewer
capabilities (`led_action`, `device_blacklist`, `diagnostics`, `device_usage`, `block_apps`,
`ddns_enabled`, `historical_usage`, the `dnsfilter_*` entries and more are missing). Under the
new string the network's `resources` map also points `forwards`, `routing` and `support` at API
`2.3`; `get_forwards`, `get_routing` and `get_support` pin their requests to `2.2`, so their
response shape does not change. To reproduce the previous behaviour:

```rust
use rusteero::consts::LEGACY_USER_AGENT;

let client = Client::builder()
    .session(Some(session))
    .user_agent(Some(LEGACY_USER_AGENT.to_owned()))
    .build()
    .await?;
assert_eq!(client.api().transport().user_agent(), LEGACY_USER_AGENT);
```

The session refresh runs on the client's own transport, so it carries the same string.
`LoginFlow::new(..)` sends the default; `LoginFlow::with_transport(transport)` sends whatever that
transport was built with (`Transport::builder().user_agent(..)`).

### Transport options

| Option | What it does |
|---|---|
| `send_legacy_cookie(bool)` | `true` (default) sends `Cookie: s=<token>` alongside the primary `X-User-Token` header; `false` sends the header only. Both are attached only when the request URL has no userinfo and its scheme, host and effective port match the configured API host (an explicit default port counts as none) — a request anywhere else goes out without a credential and logs a `WARN` naming only the reason and the refused `scheme://host`. A URL carrying userinfo is refused before sending with `Error::Validation { field: "url" }` |
| `accept_language(..)` | The `X-Accept-Language` header on every request. Printable ASCII with no CR/LF, checked at `build()` (`field: "X-Accept-Language"`) |
| `user_agent(Option<String>)` | The `User-Agent` header; `None` sends `consts::DEFAULT_USER_AGENT`. Same validation (`field: "User-Agent"`); read the effective value back with `Transport::user_agent()` |
| `get_retries(u32)` | Additional attempts for a `GET` that fails with `Error::Network`, `Error::Timeout` or any classified error with status `>= 500` except a rate limit, 500 ms apart, each logged at `WARN`. Writes are never retried; `4xx`, `429` and a 5xx carrying `error.rate.limit` are never retried; the one-shot 401 refresh-and-replay is separate |
| `base_url(..)` | Replaces the API host for **both** `/2.2` and `/2.3`; also what link resolution and the credential gate compare against. Meant for a local mock server in tests. Must be an absolute URL with a scheme and a host, without userinfo and with an authority that parses, else `Error::Validation { field: "base_url" }` at `build()` with one of the fixed messages `must be an absolute URL with a scheme and a host`, `must not carry userinfo` or `has an authority that cannot be parsed` (the value is never echoed). `""` is rejected; "no override" is not calling the method |
| `http_builder(reqwest::ClientBuilder)` | Your own HTTP client builder. The crate applies `reqwest::redirect::Policy::none()` on top before building. Its default timeouts do not apply: set `.timeout(..)` / `.read_timeout(..)` on the builder yourself if you need them |

**Timeouts.** The defaults are 30 s total and 10 s per read (`consts::REQUEST_TIMEOUT`,
`consts::READ_TIMEOUT`), enforced by the crate itself on every attempt: the total bound covers
sending the request and reading the (size-capped) body, the read bound covers the wait for the
response head (which includes connecting and writing the request) and then each wait for the
next body chunk. `ClientBuilder` has no timeout setters; `Transport::builder()` exposes
`timeout(..)` and `read_timeout(..)` for a caller building an `EeroApi` by hand.
`Duration::ZERO` on either **switches that bound off** (unbounded) rather than timing out at
once. Both are ignored when `http_builder(..)` is supplied: the transport default is then
unbounded and the builder's own timeouts govern.

**Per-call timeouts.** `rusteero::RequestTimeout { total, read }` replaces the transport default
for one request: the ten time-windowed data-usage reads take it as `timeout:
Option<RequestTimeout>`, and `Transport::{request, resource, nested}_with_timeout` take it for a
raw call. A supplied value replaces **both** bounds wholesale; a field left `None` or set to
`Duration::ZERO` is unbounded and never falls back to the default (`RequestTimeout::total(d)` has
no read bound; `RequestTimeout::default()` has none at all). Pass `None` instead of a
`RequestTimeout` to keep the default. Each attempt of a `GET` retry gets the full budget, the
replay after an `error.session.refresh` 401 keeps the per-call value, and the refresh `POST`
itself always uses the transport default. Under `http_builder(..)` a per-call `total` replaces the
builder's total timeout, but a `read_timeout` set on the builder still caps every request. A
request that times out closes its connection instead of returning it to the pool.

Every request also carries `Accept: application/json`; `Content-Type` is set per request by the
body encoding (JSON, form, or the literal `""`).

Response bodies are capped at 10 MiB (`consts::MAX_RESPONSE_BYTES`); a `3xx` is refused before
the body is read; an empty or whitespace-only `2xx` body becomes an empty `Envelope`. A `2xx` body
that is valid JSON but not an object is `Error::Api` with `Unexpected JSON response: expected an
object`, and one that is not UTF-8 is `Error::Api` with `Response body is not valid UTF-8`. A
non-2xx body is decoded lossily, so it is still classified by its status and `meta.error`.

A request URL that carries userinfo (`user:pass@host`) is refused before anything is sent
(`Error::Validation { field: "url", message: "must not carry userinfo" }`): reqwest would turn it
into Basic credentials.

### Disabling the keyring for headless use

```toml
[dependencies]
rusteero = { version = "3", default-features = false }
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

*   `ERROR` (transport): `Request timed out on <METHOD> request`; `Network error (<kind>) on
    <METHOD> request`, where `<kind>` is a fixed token (`connect`, `timeout`, `request`, `body`,
    `decode`, `redirect`, `other`), never reqwest's own text; and, for a `2xx` body the crate
    cannot use, `Undecodable response body (<n> bytes)`, `Unexpected <list|str|int|float|bool|NoneType>
    in JSON response (<n> bytes)` or `Error parsing JSON response (<n> bytes): …` (the parser's
    position only, never the body).
*   `WARN` (transport): a GET retry (`Retrying GET request after transient failure (attempt
    N/M)`), a refused redirect (`Redirect response <status> blocked`), a credential withheld from
    an off-origin request (`Session header withheld from a request to a <reason>: <scheme://host>`),
    and, under
    `StorageFailures::Warn`, a failure of a store that does not log its own failures
    (`failed to persist session to credential store` for a save, `failed to clear session from
    credential store` for a clear, with the store's error as a field). The crate's own
    `FileStore`, `ChainedStore` and `KeyringStore` (with `warn_on_unpersisted` on) report their
    failures themselves (`CredentialStore::reports_failures`), so the transport records those at
    `DEBUG` and each failure appears exactly once (see [`StorageFailures`](#storagefailures)).
*   `ERROR` (transport): `API error <status> on <method> request` for every non-success status
    except a refused 3xx (see the redirect `WARN`), 401, 404 and 429, once per attempt and with no
    URL.
*   `WARN` (elsewhere): the stores' own fixed lines for a save that was not kept (see
    [Credential stores](#credential-stores)), a `ChainedStore::clear` that left a backend
    uncleared, a `FileStore` file found group- or other-accessible when read (Unix; the line names
    the file's path), a
    store that failed its initial load in `ClientBuilder::build()` under `StorageFailures::Warn`
    (`failed to load initial session from credential store`), `logout()` with no session
    (`Attempted to logout when not authenticated`), a failed `logout` request (`Logout API call
    failed: …`), a legacy-record migration mismatch, the deprecation notice whenever
    `reporter_data` is `Some` and not JSON `null`, and one fixed line before every uncharacterised write (see
    [Rust API — Writes and safety](Rust-API#writes-and-safety)).
*   `DEBUG`: method, path and status of every request (the path only, never the query string),
    the path of a timed-out, failed or retried request and the host and path of a refused
    redirect's `Location`; for an error response, the parsed envelope **after**
    `rusteero::redact::redact_sensitive` has replaced sensitive keys (tokens, passwords,
    identifier-shaped values). The redaction runs only when `DEBUG` is enabled. Never headers,
    never cookies, never raw body text.

**No `ERROR` or `WARN` line from the transport carries a request URL, a path or a query
string** (the credential-withheld line names only the refused `scheme://host`): the network id,
a device MAC or a profile id in the path can only appear at `DEBUG`. No line at any level carries
a credential.

`redact_sensitive` walks objects and arrays, a top-level array included. A container nested 16
levels deep or more (the argument itself is level 0) is replaced wholesale by the fixed string
`[REDACTED:cyclic-or-too-deep]`, so a pathological value cannot exhaust the stack or hide a
sensitive key below the point where the redactor stops looking.

Store-level lines are logged on Tokio's blocking pool but recorded under the dispatcher and span
that were current when the store call was made, so a subscriber scoped to the calling task
(`tracing::subscriber::with_default`, a per-task dispatcher) sees them.

Error messages are fixed labels, never built from a response body; `Error::envelope()` holds the
raw response if you need it. Two exceptions to keep in mind before logging an error's text: a
refused redirect's `Error::Api` message names the request path and the `Location` host
(`Redirect not followed: <status> at <path> -> <host>`), and an `Error::Network`'s text embeds
reqwest's own message, which includes the full request URL, query string included (the
`Logout API call failed: …` warning prints that text for a network failure). If you log an envelope yourself, pass it
through `redact_sensitive` first — a network envelope carries the Wi-Fi password.

## Security properties

*   Session tokens are `secrecy::SecretString`; they never appear in `Debug` output, logs or
    error messages. `Envelope`'s `Debug` prints only `meta.code` and the *shape* of `data`.
*   The credential goes only to the configured API origin (scheme, host and effective port
    compared, and never to a URL with userinfo), and redirects are refused
    (`reqwest::redirect::Policy::none()`), so a `3xx` can never carry the token elsewhere. A
    caller-supplied `reqwest::ClientBuilder` gets the same policy applied before it is built, so
    an injected client cannot follow one either.
*   A bare id must be a single path segment (`[A-Za-z0-9][A-Za-z0-9._:-]*`, no `..`); a link
    value read from an envelope must be host-relative; an absolute URL must be on the API origin
    with no userinfo. A path or URL a caller supplies must also be a plain path (no query,
    fragment, dot segment, percent-encoded dot, whitespace, control character or backslash) and
    name exactly one resource of the family the method addresses, so `/2.2/account` cannot be
    passed where a network is expected. All of this is checked before a request is built; see
    [Rust API — Ids, paths, URLs and `parent`](Rust-API#ids-paths-urls-and-parent).
*   `FileStore` writes are atomic and `0600` from the moment the file exists (Unix).
*   Every write that has not been characterised against a live network logs a warning first.

---

## 🔗 Related Pages

*   [📖 Rust API](Rust-API)
*   [🔧 Troubleshooting](Troubleshooting)
*   [🔀 Migration](Migration)
*   [🏠 Home](Home)
