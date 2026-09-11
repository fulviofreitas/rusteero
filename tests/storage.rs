//! P2 storage suite: the credential-store backends (`FileStore`, `ChainedStore`,
//! `create_storage`'s four-way matrix) plus the storage-adjacent half of `Transport`
//! (`StorageFailures`, `set_session`/`refresh_session` persistence), all exercised from outside
//! the crate the way a real consumer would.
//!
//! The headline requirement this file exists to satisfy is the Phase 2 exit criterion: **"Storage
//! format round-trips with a real eero-api cookies.json fixture."** `fixtures/cookies.json` and
//! `fixtures/cookies_legacy.json` are not derived from this crate's own `Session`/`FileStore`
//! code — their shape is taken from `.claude/tasks/briefs/const.md`'s line-cited behaviour brief
//! for `eero-api`'s `AuthCredentials.to_dict()`/`from_dict()` (`auth_storage.py:50-80`), so a
//! green test here is evidence this port can read what the Python library actually writes, not
//! merely what this port itself writes.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, fixture};
use rusteero::auth::Session;
use rusteero::error::{Error, StorageError};
use rusteero::storage::{
    ChainedStore, CredentialStore, FileStore, MemoryStore, StorageConfig, create_storage,
};
use rusteero::transport::{StorageFailures, Transport};

/// Copies `fixture_name`'s exact, checked-in bytes from `tests/fixtures/` into a fresh
/// `cookies.json` inside `dir`, returning the new path.
///
/// Every storage test in this file that needs a file on disk goes through this (or writes
/// directly into a `tempfile::tempdir()`), per `.claude/rules/testing.md`'s "Never write outside
/// a temp dir" rule — `FileStore` would happily overwrite the checked-in fixture itself if a test
/// pointed it there directly.
fn copy_fixture_into(dir: &Path, fixture_name: &str) -> PathBuf {
    let path = dir.join("cookies.json");
    std::fs::write(&path, fixture(fixture_name)).expect("write into a fresh tempdir succeeds");
    path
}

// ===================== D-5 contract: FileStore reads a real eero-api cookies.json =====================

#[test]
fn filestore_loads_the_real_eero_api_cookies_json_fixture() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = copy_fixture_into(dir.path(), "cookies.json");
    let store = FileStore::new(&path);

    let session = store.load()?;

    assert_eq!(session.token().expose_secret(), "session_valid_id_7f3a9c2e");
    assert_eq!(
        session
            .refresh_token()
            .expect("fixtures/cookies.json carries a refresh_token")
            .expose_secret(),
        "rt_refresh_token_4b8d1f56"
    );
    assert!(
        session.expiry().is_some(),
        "fixtures/cookies.json carries a future session_expiry"
    );
    assert!(
        session.is_valid(),
        "a far-future expiry with a non-empty token must be a valid session"
    );
    Ok(())
}

#[test]
fn filestore_loads_the_legacy_user_token_fixture() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = copy_fixture_into(dir.path(), "cookies_legacy.json");
    let store = FileStore::new(&path);

    let session = store.load()?;

    assert_eq!(
        session.token().expose_secret(),
        "legacy_user_token_c1a2b3d4"
    );
    assert!(
        session.refresh_token().is_none(),
        "the legacy fixture never carried a refresh token"
    );
    assert!(session.is_valid());
    Ok(())
}

#[test]
fn filestore_save_then_reload_preserves_every_field() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let store = FileStore::new(dir.path().join("cookies.json"));
    let original = Session::from_json(
        r#"{"session_id":"round-trip-token","refresh_token":"round-trip-refresh","session_expiry":"2099-06-15T09:30:00"}"#,
    )?;

    store.save(&original)?;
    let reloaded = store.load()?;

    assert_eq!(
        reloaded.token().expose_secret(),
        original.token().expose_secret()
    );
    assert_eq!(
        reloaded.refresh_token().map(ExposeSecret::expose_secret),
        original.refresh_token().map(ExposeSecret::expose_secret)
    );
    assert_eq!(reloaded.expiry(), original.expiry());
    Ok(())
}

// ===================== D-5 contract: expiry format regression guard =====================

/// Checks `s` matches `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$` exactly — no regex dependency
/// needed for a fixed, known-length shape. Written independently of `src/auth/session.rs`'s own
/// unit test of the same shape (rather than imported), so this integration test does not rely on
/// the library's own test helper to prove the same guarantee it is itself supposed to catch a
/// regression in.
fn is_naive_local_iso8601(s: &str) -> bool {
    let bytes = s.as_bytes();
    s.len() == 19
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && s[0..4].bytes().all(|b| b.is_ascii_digit())
        && s[5..7].bytes().all(|b| b.is_ascii_digit())
        && s[8..10].bytes().all(|b| b.is_ascii_digit())
        && s[11..13].bytes().all(|b| b.is_ascii_digit())
        && s[14..16].bytes().all(|b| b.is_ascii_digit())
        && s[17..19].bytes().all(|b| b.is_ascii_digit())
}

#[test]
fn filestore_save_writes_expiry_as_naive_nineteen_char_iso8601() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("cookies.json");
    let store = FileStore::new(&path);

    store.save(&Session::from_token("tok"))?;

    let raw = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    let expiry = value["session_expiry"]
        .as_str()
        .expect("session_expiry is a string");

    assert_eq!(expiry.len(), 19, "expected exactly 19 characters: {expiry}");
    assert!(
        is_naive_local_iso8601(expiry),
        "expiry {expiry} does not match ^\\d{{4}}-\\d{{2}}-\\d{{2}}T\\d{{2}}:\\d{{2}}:\\d{{2}}$"
    );
    assert!(!expiry.contains('.'), "must have no fractional seconds");
    assert!(!expiry.contains('Z'), "must have no UTC designator");
    assert!(!expiry.contains('+'), "must have no UTC offset");
    Ok(())
}

// ===================== 0600 permissions (Unix) =====================

#[cfg(unix)]
#[test]
fn filestore_save_creates_owner_only_permissions_from_an_external_crate() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("cookies.json");
    let store = FileStore::new(&path);

    store.save(&Session::from_token("tok"))?;

    let mode = std::fs::metadata(&path)?.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "expected owner-only permissions, got {mode:o}");
    Ok(())
}

// ===================== ChainedStore, exercised against real backends =====================

#[test]
fn chained_store_load_prefers_primary_over_a_fallback_that_would_error_if_touched()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let primary_path = dir.path().join("primary.json");
    let primary_file = FileStore::new(&primary_path);
    primary_file.save(&Session::from_token("primary-token"))?;

    // A fallback pointed at a directory, not a file: any attempt to read it as a cookie file
    // fails with a real `io::Error` — if this test ever turns red, that is proof
    // `ChainedStore::load` consulted the fallback despite a usable primary session.
    let fallback_dir = dir.path().join("this-is-a-directory-not-a-cookie-file");
    std::fs::create_dir(&fallback_dir)?;

    let chained = ChainedStore::new(
        Arc::new(primary_file) as Arc<dyn CredentialStore>,
        Arc::new(FileStore::new(&fallback_dir)) as Arc<dyn CredentialStore>,
    );
    let session = chained.load()?;

    assert_eq!(session.token().expose_secret(), "primary-token");
    Ok(())
}

#[test]
fn chained_store_load_falls_back_and_migrates_into_a_real_primary_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let primary_path = dir.path().join("primary.json"); // does not exist yet
    let primary_file = Arc::new(FileStore::new(&primary_path));

    let fallback_path = dir.path().join("fallback.json");
    let fallback_file = Arc::new(FileStore::new(&fallback_path));
    fallback_file.save(&Session::from_token("fallback-token"))?;

    let chained = ChainedStore::new(
        Arc::clone(&primary_file) as Arc<dyn CredentialStore>,
        Arc::clone(&fallback_file) as Arc<dyn CredentialStore>,
    );
    let session = chained.load()?;
    assert_eq!(session.token().expose_secret(), "fallback-token");

    assert!(
        primary_path.exists(),
        "a usable fallback session must be migrated into primary"
    );
    let migrated = primary_file.load()?;
    assert_eq!(migrated.token().expose_secret(), "fallback-token");
    Ok(())
}

#[test]
fn chained_store_save_falls_back_to_a_working_store_when_the_primary_directory_cannot_be_created()
-> anyhow::Result<()> {
    // A regular file standing where the primary's parent directory would need to be created:
    // `fs::create_dir_all` on this path fails with a real `io::Error` — a genuine backend
    // failure, not a test double, exercising the documented divergence from `eero-api`
    // (`.claude/tasks/briefs/const.md` §7, gotcha #7): unlike Python's `KeyringStorage.save()`,
    // which never raises, this port's stores can surface a real error, so `ChainedStore`'s
    // fallback branch actually activates instead of being dead code.
    let dir = tempfile::tempdir()?;
    let blocking_file = dir.path().join("not-a-directory");
    std::fs::write(&blocking_file, b"blocking")?;
    let primary_path = blocking_file.join("nested").join("cookies.json");

    let fallback_path = dir.path().join("fallback.json");
    let fallback_file = Arc::new(FileStore::new(&fallback_path));

    let chained = ChainedStore::new(
        Arc::new(FileStore::new(&primary_path)) as Arc<dyn CredentialStore>,
        Arc::clone(&fallback_file) as Arc<dyn CredentialStore>,
    );
    chained.save(&Session::from_token("fallback-only-token"))?;

    assert!(
        !primary_path.exists(),
        "the primary write must have genuinely failed, not silently succeeded"
    );
    let saved = fallback_file.load()?;
    assert_eq!(saved.token().expose_secret(), "fallback-only-token");
    Ok(())
}

// ===================== create_storage's four-way matrix =====================

#[cfg(feature = "keyring")]
mod create_storage_matrix {
    use secrecy::ExposeSecret;

    use super::{Session, StorageConfig, create_storage};

    #[test]
    fn neither_flag_set_yields_a_working_memory_store() -> anyhow::Result<()> {
        let store = create_storage(&StorageConfig::default());
        assert!(format!("{store:?}").contains("MemoryStore"));
        store.save(&Session::from_token("tok"))?;
        assert_eq!(store.load()?.token().expose_secret(), "tok");
        Ok(())
    }

    #[test]
    fn cookie_file_only_yields_a_working_file_store() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("cookies.json");
        let store = create_storage(&StorageConfig {
            use_keyring: false,
            cookie_file: Some(path.clone()),
        });
        assert!(format!("{store:?}").contains("FileStore"));
        store.save(&Session::from_token("tok"))?;
        assert!(path.exists());
        assert_eq!(store.load()?.token().expose_secret(), "tok");
        Ok(())
    }

    #[test]
    fn keyring_only_yields_a_bare_keyring_store_with_no_file_fallback() {
        let store = create_storage(&StorageConfig {
            use_keyring: true,
            cookie_file: None,
        });
        let debug = format!("{store:?}");
        assert!(debug.contains("KeyringStore"));
        assert!(!debug.contains("ChainedStore"));
        assert!(!debug.contains("FileStore"));
    }

    #[test]
    fn keyring_and_cookie_file_yields_a_chained_store() {
        let store = create_storage(&StorageConfig {
            use_keyring: true,
            cookie_file: Some(std::path::PathBuf::from(
                "/tmp/rusteero-storage-test-unused-cookie-file.json",
            )),
        });
        let debug = format!("{store:?}");
        assert!(debug.contains("ChainedStore"));
        assert!(debug.contains("KeyringStore"));
        assert!(debug.contains("FileStore"));
    }
}

#[cfg(not(feature = "keyring"))]
mod create_storage_matrix {
    use secrecy::ExposeSecret;

    use super::{Session, StorageConfig, create_storage};

    #[test]
    fn neither_flag_set_yields_a_working_memory_store() -> anyhow::Result<()> {
        let store = create_storage(&StorageConfig::default());
        assert!(format!("{store:?}").contains("MemoryStore"));
        store.save(&Session::from_token("tok"))?;
        assert_eq!(store.load()?.token().expose_secret(), "tok");
        Ok(())
    }

    #[test]
    fn cookie_file_only_yields_a_working_file_store() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("cookies.json");
        let store = create_storage(&StorageConfig {
            use_keyring: false,
            cookie_file: Some(path.clone()),
        });
        assert!(format!("{store:?}").contains("FileStore"));
        store.save(&Session::from_token("tok"))?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn keyring_requested_with_no_file_degrades_to_a_working_memory_store() -> anyhow::Result<()> {
        let store = create_storage(&StorageConfig {
            use_keyring: true,
            cookie_file: None,
        });
        assert!(format!("{store:?}").contains("MemoryStore"));
        store.save(&Session::from_token("tok"))?;
        assert_eq!(store.load()?.token().expose_secret(), "tok");
        Ok(())
    }

    #[test]
    fn keyring_requested_with_a_file_degrades_to_a_working_file_store() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("cookies.json");
        let store = create_storage(&StorageConfig {
            use_keyring: true,
            cookie_file: Some(path.clone()),
        });
        assert!(format!("{store:?}").contains("FileStore"));
        store.save(&Session::from_token("tok"))?;
        assert!(path.exists());
        Ok(())
    }
}

// ===================== Expired session leaves a Transport unauthenticated =====================

#[test]
fn expired_session_loaded_from_a_store_leaves_a_transport_unauthenticated() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("cookies.json");
    // Mirrors the shape of eero-api's own `expired_session_data` test fixture
    // (`.claude/tasks/briefs/const.md`, "Fixture payloads"), with a fixed past date instead of a
    // wall-clock-relative one so this test can never become flaky.
    std::fs::write(
        &path,
        r#"{"session_id": "session_expired_id", "refresh_token": "rt_expired_refresh", "session_expiry": "2000-01-01T00:00:00"}"#,
    )?;
    let store = FileStore::new(&path);

    let session = store.load()?;
    assert!(
        !session.is_valid(),
        "an expiry in the past must never be valid"
    );

    let transport = Transport::builder().session(Some(session)).build()?;
    assert!(!transport.is_authenticated());
    Ok(())
}

// ===================== StorageFailures policy =====================

/// A [`CredentialStore`] that fails every operation — used only to exercise
/// [`StorageFailures`]'s two policies from outside the crate. Every other test in this file
/// drives a genuine backend instead.
#[derive(Debug, Default)]
struct AlwaysFailsStore;

impl CredentialStore for AlwaysFailsStore {
    fn load(&self) -> Result<Session, StorageError> {
        Err(StorageError::Backend {
            backend: "always-fails".to_owned(),
            message: "deliberate integration-test failure".to_owned(),
        })
    }

    fn save(&self, _session: &Session) -> Result<(), StorageError> {
        Err(StorageError::Backend {
            backend: "always-fails".to_owned(),
            message: "deliberate integration-test failure".to_owned(),
        })
    }

    fn clear(&self) -> Result<(), StorageError> {
        Err(StorageError::Backend {
            backend: "always-fails".to_owned(),
            message: "deliberate integration-test failure".to_owned(),
        })
    }
}

#[test]
fn storage_failures_warn_lets_set_session_proceed_despite_a_failing_store() -> anyhow::Result<()> {
    let store: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
    let transport = Transport::builder()
        .session(Some(Session::from_token("tok")))
        .store(Some(store))
        .storage_failures(StorageFailures::Warn)
        .build()?;

    transport.set_session(None)?;
    assert!(
        transport.session().is_none(),
        "the in-memory session updates regardless of the store's outcome"
    );
    Ok(())
}

#[test]
fn storage_failures_fatal_returns_error_storage_when_the_store_fails() -> anyhow::Result<()> {
    let store: Arc<dyn CredentialStore> = Arc::new(AlwaysFailsStore);
    let transport = Transport::builder()
        .session(Some(Session::from_token("tok")))
        .store(Some(store))
        .storage_failures(StorageFailures::Fatal)
        .build()?;

    let err = transport
        .set_session(None)
        .expect_err("the store always fails");
    assert!(matches!(err, Error::Storage(_)));
    // Security finding F3 (`src/transport.rs`): even under the Fatal policy, the in-memory
    // session must still update before `set_session` returns.
    assert!(transport.session().is_none());
    Ok(())
}

// ===================== MockEero + a real Transport operation persists into the store =====================

#[tokio::test]
async fn transport_with_store_set_session_persists_the_new_session() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = mock.transport_with_store("initial-token", Arc::clone(&store));

    transport.set_session(Some(Session::from_token("rotated-token")))?;

    let stored = store.load()?;
    assert_eq!(stored.token().expose_secret(), "rotated-token");
    Ok(())
}

#[tokio::test]
async fn transport_refresh_session_persists_the_refreshed_session_into_the_store()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            json!({
                "meta": { "code": 200 },
                "data": { "session_token": "refreshed-token", "refresh_token": "refreshed-refresh" }
            })
            .to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let session = Session::from_json(
        r#"{"session_id":"about-to-be-refreshed","refresh_token":"pre-refresh-token","session_expiry":"2099-01-01T00:00:00"}"#,
    )?;
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(session))
        .store(Some(Arc::clone(&store)))
        .build()?;

    // Sanity: nothing has been persisted yet — only `refresh_session`'s own persistence point
    // (the behaviour under test) should ever populate the store.
    assert!(!store.load()?.is_valid());

    let refreshed = transport.refresh_session().await?;
    assert!(
        refreshed,
        "the mocked refresh response carries a non-empty session_token"
    );

    let stored = store.load()?;
    assert_eq!(stored.token().expose_secret(), "refreshed-token");
    Ok(())
}
