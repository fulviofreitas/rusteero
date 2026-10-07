//! Storage suite: the credential-store backends (`FileStore`, `ChainedStore`, `create_storage`'s
//! four-way matrix), the `v8.0.4` legacy-record migration path, plus the storage-adjacent half of
//! `Transport` (`StorageFailures`, `set_session`/`refresh_session` persistence), all exercised
//! from outside the crate the way a real consumer would.
//!
//! `fixtures/cookies.json`, `fixtures/cookies_legacy.json`, and `fixtures/cookies_eeroctl.json`
//! are all **legacy** records (no `schema_version` key) — loading any of them exercises the
//! re-save-and-read-back migration path every backend with a load path applies.

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
fn copy_fixture_into(dir: &Path, fixture_name: &str) -> PathBuf {
    let path = dir.join("cookies.json");
    std::fs::write(&path, fixture(fixture_name)).expect("write into a fresh tempdir succeeds");
    path
}

// ===================== Shared wire contract: FileStore reads a real eero-api cookies.json =====================

#[test]
fn filestore_loads_a_real_eeroctl_cookies_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = copy_fixture_into(dir.path(), "cookies_eeroctl.json");
    let store = FileStore::new(&path);

    let session = store.load()?;

    assert_eq!(session.token().expose_secret().len(), 35);
    assert!(session.is_valid());
    Ok(())
}

#[test]
fn filestore_loads_the_legacy_eero_api_cookies_json_fixture_and_migrates_it() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let path = copy_fixture_into(dir.path(), "cookies.json");
    let store = FileStore::new(&path);

    let session = store.load()?;

    assert_eq!(session.token().expose_secret(), "session_valid_id_7f3a9c2e");
    assert!(session.is_valid());

    // The migration must have re-saved the file in the current (v8.0.4) schema shape.
    let on_disk = std::fs::read_to_string(&path)?;
    assert!(on_disk.contains("\"schema_version\":2"));
    assert!(!on_disk.contains("refresh_token"));
    assert!(!on_disk.contains("session_expiry"));
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
    assert!(session.is_valid());
    Ok(())
}

#[test]
fn filestore_save_then_reload_round_trips_the_token() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let store = FileStore::new(dir.path().join("cookies.json"));
    let original = Session::from_token("round-trip-token");

    store.save(&original)?;
    let reloaded = store.load()?;

    assert_eq!(
        reloaded.token().expose_secret(),
        original.token().expose_secret()
    );
    Ok(())
}

/// A legacy fixture carrying an expiry far in the past is now unconditionally valid — `v8.0.4`
/// has no client-side expiry concept at all, so the (dropped) `session_expiry` field can no
/// longer make a token look expired.
#[test]
fn a_legacy_expired_session_expiry_field_no_longer_matters() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("cookies.json");
    std::fs::write(
        &path,
        r#"{"session_id": "session_expired_id", "refresh_token": "rt_expired_refresh", "session_expiry": "2000-01-01T00:00:00"}"#,
    )?;
    let store = FileStore::new(&path);

    let session = store.load()?;
    assert!(
        session.is_valid(),
        "v8.0.4 has no client-side expiry concept: a non-empty token is always valid locally"
    );

    let transport = Transport::builder().session(Some(session)).build()?;
    assert!(transport.is_authenticated());
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
    let primary_path = dir.path().join("primary.json");
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

    assert!(primary_path.exists());
    let migrated = primary_file.load()?;
    assert_eq!(migrated.token().expose_secret(), "fallback-token");
    Ok(())
}

#[test]
fn chained_store_save_falls_back_to_a_working_store_when_the_primary_directory_cannot_be_created()
-> anyhow::Result<()> {
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

    assert!(!primary_path.exists());
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

// ===================== StorageFailures policy =====================

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
    assert!(transport.session().is_none());
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
async fn transport_refresh_session_success_never_touches_the_store() -> anyhow::Result<()> {
    // A successful refresh never rotates the token (server-issued token discarded per SDK
    // policy), so there is nothing new to persist — the store must be left exactly as it was.
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({ "meta": { "code": 200 }, "data": {} }).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = mock.transport_with_store("about-to-be-refreshed", Arc::clone(&store));

    assert!(
        !store.load()?.is_valid(),
        "sanity: seeding a Transport's session does not itself persist anything"
    );

    let refreshed = transport.refresh_session().await?;
    assert!(refreshed);

    assert!(
        !store.load()?.is_valid(),
        "a successful refresh must not write anything to the store"
    );
    Ok(())
}

#[tokio::test]
async fn transport_refresh_session_terminal_failure_clears_the_store() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.revoked"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    store.save(&Session::from_token("about-to-be-cleared"))?;
    let transport = mock.transport_with_store("about-to-be-cleared", Arc::clone(&store));

    let refreshed = transport.refresh_session().await?;
    assert!(!refreshed);
    assert!(!store.load()?.is_valid());
    Ok(())
}
