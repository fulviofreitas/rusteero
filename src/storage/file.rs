//! Filesystem credential store.
//!
//! Ported from `FileStorage` (`eero-api`'s `src/eero/api/auth_storage.py:169-237`). See
//! `rusteero-context/claude/tasks/briefs/const.md` §5 for the full behaviour brief this module
//! implements, and `security-review.md`'s rule for this repo for the one deliberate improvement
//! over the Python original (atomic, mode-on-create file permissions instead of a
//! write-then-`chmod` sequence).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::auth::Session;
use crate::error::StorageError;

use super::CredentialStore;

/// A [`CredentialStore`] backed by a single JSON file on disk.
///
/// Mirrors `FileStorage` (`auth_storage.py:169-237`): the file holds exactly the JSON object
/// `Session::to_json`/[`Session::from_json`] produce/consume (`session_id`, `refresh_token`,
/// `session_expiry`, plus the legacy `user_token` read alias) — the same shape a Python
/// `eero-api` install reads and writes, per decision D-5. There is deliberately no default
/// path: like `FileStorage.__init__`, which always requires an explicit `file_path`, choosing
/// *where* the file lives is left entirely to the caller (in `eero-api`'s ecosystem, that
/// choice belongs to the CLI layer, not this library).
///
/// # Security: atomic `0600` creation, not `chmod` after the fact
///
/// Python's `FileStorage.save()` writes the file at the process' default mode first and only
/// restricts it to owner-only (`0600`) with a separate `os.chmod()` call afterwards
/// (`auth_storage.py:220-224`) — a real, if narrow, window during which the file is readable by
/// anyone who can read the containing directory. This port closes that window: [`FileStore::save`]
/// writes to a temporary file in the same directory, created with `0600` permissions *atomically*
/// as part of the file-creation syscall (via [`std::os::unix::fs::OpenOptionsExt::mode`]), then
/// renames it into place. A crash between the write and the rename leaves either the old file or
/// nothing at all — never a partially-written or loosely-permissioned target file.
///
/// **This guarantee is Unix-only.** `OpenOptionsExt::mode` has no equivalent in the Windows
/// standard library; on non-Unix platforms the temporary file is created with whatever default
/// permissions/ACL the OS applies, and this crate does not attempt to further restrict them.
/// Callers on Windows should not rely on the stored file being owner-only — treat the containing
/// directory's own ACLs as the real access boundary there.
///
/// # Security: no orphaned plaintext credential (phase-2 storage review, finding S3)
///
/// The temporary file above is given a name that is unique for the lifetime of this process
/// (see `unique_temp_path`), and every error path in the create/write/sync/rename sequence
/// removes it before returning (see `write_private_atomically`) — a crash *between* those
/// steps is the only way a temp file can survive, and [`FileStore::clear`] proactively removes
/// any such orphan for `path` (see `remove_orphaned_temp_files`), so a stale, fully-valid
/// plaintext credential can never outlive an explicit `clear()` call.
#[derive(Debug)]
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    /// Creates a store backed by `path`, which need not exist yet.
    ///
    /// Unlike `FileStorage.__init__` (`auth_storage.py:172-178`), which eagerly expands `~` and
    /// absolutizes the path at construction time, `path` is stored exactly as given — this port
    /// has no path-expansion dependency available (decision: no new dependencies, per the phase
    /// brief) and resolving `~`/relative paths is left to the caller, consistent with there
    /// being no default path anywhere in this crate.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The path this store reads from and writes to.
    ///
    /// Mirrors the read-only `file_path` property (`auth_storage.py:180-183`).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl CredentialStore for FileStore {
    fn load(&self) -> Result<Session, StorageError> {
        match fs::read_to_string(&self.path) {
            // Mirrors the "file does not exist" branch (`auth_storage.py:188-190`): this is the
            // expected "never logged in" state, not an error.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Session::empty()),
            Err(err) => Err(StorageError::Io(err)),
            Ok(contents) => Session::from_json(&contents),
        }
    }

    fn save(&self, session: &Session) -> Result<(), StorageError> {
        // Mirrors `os.makedirs(cookie_dir, exist_ok=True)` (`auth_storage.py:215-217`); an empty
        // parent (a bare file name with no directory component) has nothing to create.
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let json = session.to_json()?;
        write_private_atomically(&self.path, json.as_bytes())
    }

    fn clear(&self) -> Result<(), StorageError> {
        let result = match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            // Mirrors the tolerant `os.path.exists(...)` guard before `os.remove`
            // (`auth_storage.py:230-237`): removing an entry that was never there is not an
            // error.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(StorageError::Io(err)),
        };
        // Security fix (phase-2 storage review, finding S3(a)): also remove any temporary file
        // `write_private_atomically` may have left behind after a crash between creating it and
        // renaming it into place — otherwise a full plaintext credential can survive at a
        // predictable path even after `clear()` reports success. Best-effort: a missing (or
        // already-removed) orphan is not an error, matching the tolerance above.
        remove_orphaned_temp_files(&self.path);
        result
    }
}

/// Writes `contents` to `path`, creating or replacing it atomically with owner-only (`0600`)
/// permissions from the moment the file is created.
///
/// Writes to a sibling temporary file first (same directory as `path`, so the final
/// [`fs::rename`] is guaranteed to be on the same filesystem and therefore atomic), then renames
/// it over `path`.
///
/// # Security (phase-2 storage review, finding S3(b))
///
/// If any step of the create/write/sync/rename sequence fails, the temporary file is removed
/// before the error is returned (see [`write_to_temp_and_rename`]) — no error path here can
/// leave a complete plaintext credential sitting at a predictable location. The one case this
/// cannot cover is a hard crash (SIGKILL, power loss) *during* that sequence, which no code
/// running in-process can react to; [`FileStore::clear`] cleans up any such orphan instead (see
/// [`remove_orphaned_temp_files`]).
fn write_private_atomically(path: &Path, contents: &[u8]) -> Result<(), StorageError> {
    let tmp_path = unique_temp_path(path);
    let result = write_to_temp_and_rename(&tmp_path, path, contents);
    if result.is_err() {
        // Best-effort: the cleanup's own outcome is discarded — the error the caller needs to
        // see is the original write failure, not a follow-up removal failure.
        let _ = fs::remove_file(&tmp_path);
    }
    result.map_err(StorageError::Io)
}

/// The create-write-sync-rename sequence [`write_private_atomically`] wraps with temp-file
/// cleanup on every error path. Kept separate so every early return via `?` funnels through one
/// `Result`, giving the caller a single place to react to *any* failure in the sequence.
fn write_to_temp_and_rename(tmp_path: &Path, path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut file = create_private_file(tmp_path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

/// A process-wide counter mixed into every temporary file name [`unique_temp_path`] produces.
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The fixed prefix shared by every temporary file [`unique_temp_path`] can ever produce for
/// `path`: `path`'s own file name followed by a `.`. [`remove_orphaned_temp_files`] uses this
/// (together with the fixed `.tmp` suffix) to recognise an orphan without needing to reconstruct
/// its exact process-id/counter middle section.
fn temp_file_prefix(path: &Path) -> String {
    let base = path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("rusteero-session"));
    format!("{}.", base.to_string_lossy())
}

/// Builds a fresh, unpredictable sibling path for the temporary file [`write_private_atomically`]
/// stages content in, by mixing the current process id and a monotonically increasing
/// in-process counter into `path`'s file name, ahead of a fixed `.tmp` suffix.
///
/// # Security (phase-2 storage review, finding S3(c))
///
/// The previous implementation used a single fixed `<name>.tmp` sibling for every save, cleaned
/// up with an unconditional `fs::remove_file` at the start of each write. Two processes (or two
/// overlapping saves in the same process) sharing one cookie file could therefore unlink each
/// other's in-flight temporary file at that exact path. Mixing in [`std::process::id`] and a
/// monotonic [`AtomicU64`] counter — both from `std`, no new dependency — makes every temporary
/// file name unique for the lifetime of this process, so two concurrent writers can never target
/// the same sibling path. Because the exact name is now unpredictable from the outside,
/// [`FileStore::clear`] cannot reconstruct it to remove an orphan left by a *different*,
/// now-dead process; instead it pattern-matches on the shared [`temp_file_prefix`]/`.tmp`
/// suffix via [`remove_orphaned_temp_files`].
fn unique_temp_path(path: &Path) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = format!(
        "{}{}-{counter}.tmp",
        temp_file_prefix(path),
        std::process::id()
    );
    path.with_file_name(file_name)
}

/// Best-effort removes every temporary file [`unique_temp_path`] may have left behind for
/// `path` — e.g. one orphaned by a crash between [`create_private_file`] and the [`fs::rename`]
/// in [`write_to_temp_and_rename`]. Every entry in `path`'s parent directory whose name starts
/// with [`temp_file_prefix`] and ends with `.tmp` is treated as such an orphan and removed; a
/// missing parent directory, or the absence of any matching entry, is not an error.
///
/// This is a directory scan rather than a single [`fs::remove_file`] on one reconstructed name
/// because [`unique_temp_path`] deliberately makes that name unpredictable from the outside (see
/// that function's docs) — the only way to find one left behind is to look for anything matching
/// the shared prefix/suffix.
fn remove_orphaned_temp_files(path: &Path) {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let prefix = temp_file_prefix(path);
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let has_tmp_extension = Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"));
        if name.starts_with(&prefix) && has_tmp_extension {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Creates `path` exclusively, with permissions restricted to the owner (`0600`) set atomically
/// as part of the creating `open` call — never via a separate `chmod` afterwards.
///
/// [`std::os::unix::fs::OpenOptionsExt::mode`] only takes effect when the call actually creates
/// the file, which `create_new(true)` guarantees (it fails with `AlreadyExists` rather than
/// silently opening/truncating an existing file), so there is no window in which the file exists
/// with looser-than-`0600` permissions.
#[cfg(unix)]
fn create_private_file(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// Non-Unix fallback: creates `path` exclusively with the platform's default permissions.
///
/// There is no standard-library equivalent of Unix's create-time `mode` on this platform; see
/// [`FileStore`]'s own doc comment for what this means for callers on Windows.
#[cfg(not(unix))]
fn create_private_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;

    use secrecy::ExposeSecret;

    use super::{CredentialStore, FileStore};
    use crate::auth::Session;
    use crate::error::StorageError;

    // ===================== Round trip =====================

    #[test]
    fn save_then_load_round_trips_the_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileStore::new(dir.path().join("cookies.json"));
        let session = Session::from_token("tok-123");

        store.save(&session).expect("save succeeds");
        let loaded = store.load().expect("load succeeds");

        assert_eq!(loaded.expose_token(), "tok-123");
        assert!(loaded.is_valid());
    }

    #[test]
    fn round_trip_through_eero_api_cookies_json_shape() {
        // The exact on-disk shape `FileStorage.save()` writes (`auth_storage.py:220-221`): key
        // order `session_id`, `refresh_token`, `session_expiry`, with Python's default
        // `json.dump` separators (a space after `:`/`,`). This differs byte-for-byte from this
        // port's compact `serde_json` output, but both are valid JSON over the same D-5 wire
        // contract, so a file a Python install wrote must still load cleanly here.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let python_written = r#"{"session_id": "session_valid_id", "refresh_token": "rt_refresh_token", "session_expiry": "2026-10-10T14:32:07"}"#;
        fs_write(&path, python_written);

        let store = FileStore::new(&path);
        let session = store.load().expect("load succeeds");

        assert_eq!(session.expose_token(), "session_valid_id");
        assert_eq!(
            session
                .refresh_token()
                .expect("refresh token present")
                .expose_secret(),
            "rt_refresh_token"
        );
        assert!(session.expiry().is_some());

        // Re-saving through this port must remain a valid credential file: reloading it here
        // must still recover every field.
        store.save(&session).expect("save succeeds");
        let reloaded = store.load().expect("load succeeds");
        assert_eq!(reloaded.expose_token(), "session_valid_id");
    }

    // ===================== 0600 permissions (Unix) =====================

    #[cfg(unix)]
    #[test]
    fn save_creates_the_file_with_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let store = FileStore::new(&path);

        store
            .save(&Session::from_token("tok"))
            .expect("save succeeds");

        let mode = std::fs::metadata(&path)
            .expect("file exists")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "expected owner-only permissions, got {mode:o}");
    }

    #[cfg(unix)]
    #[test]
    fn overwriting_an_existing_looser_file_still_ends_up_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        // Simulate a pre-existing file with world-readable permissions (e.g. one written by an
        // older, non-atomic implementation).
        fs_write(&path, "{}");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("chmod succeeds");

        let store = FileStore::new(&path);
        store
            .save(&Session::from_token("tok"))
            .expect("save succeeds");

        let mode = std::fs::metadata(&path)
            .expect("file exists")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "expected save() to tighten permissions, got {mode:o}"
        );
    }

    // ===================== Parent directory creation =====================

    #[test]
    fn save_creates_missing_parent_directories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("dirs").join("cookies.json");
        let store = FileStore::new(&path);

        store
            .save(&Session::from_token("tok"))
            .expect("save succeeds");

        assert!(path.exists());
    }

    // ===================== Missing file / empty session =====================

    #[test]
    fn load_missing_file_returns_an_empty_session_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileStore::new(dir.path().join("does-not-exist.json"));

        let session = store.load().expect("missing file is not an error");

        assert!(!session.is_valid());
        assert_eq!(session.expose_token(), "");
    }

    // ===================== Malformed JSON =====================

    #[test]
    fn load_malformed_json_returns_a_storage_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        fs_write(&path, "not json");
        let store = FileStore::new(&path);

        let err = store.load().expect_err("malformed json is an error");

        assert!(matches!(err, StorageError::Serde(_)));
    }

    // ===================== Legacy `user_token` key =====================

    #[test]
    fn load_accepts_the_legacy_user_token_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        fs_write(&path, r#"{"user_token":"old_token_123","session_id":null}"#);
        let store = FileStore::new(&path);

        let session = store.load().expect("load succeeds");

        assert_eq!(session.expose_token(), "old_token_123");
    }

    // ===================== clear() =====================

    #[test]
    fn clear_on_a_missing_file_is_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileStore::new(dir.path().join("does-not-exist.json"));

        store
            .clear()
            .expect("clearing a missing file is not an error");
    }

    #[test]
    fn clear_removes_an_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let store = FileStore::new(&path);
        store
            .save(&Session::from_token("tok"))
            .expect("save succeeds");
        assert!(path.exists());

        store.clear().expect("clear succeeds");

        assert!(!path.exists());
    }

    // ===================== clear(): finding S3 =====================

    #[test]
    fn clear_removes_a_stale_temporary_file_left_by_a_crashed_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let store = FileStore::new(&path);
        store
            .save(&Session::from_token("tok"))
            .expect("save succeeds");
        // Simulate the temporary file `write_private_atomically` would have used had the
        // process been killed between creating it and renaming it into place.
        let orphan = dir.path().join("cookies.json.99999-7.tmp");
        fs_write(&orphan, "leftover plaintext credential");

        store.clear().expect("clear succeeds");

        assert!(!path.exists());
        assert!(
            !orphan.exists(),
            "clear() must remove orphaned temp files too, not just the main file"
        );
    }

    #[test]
    fn clear_with_no_file_and_no_orphan_is_still_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileStore::new(dir.path().join("does-not-exist.json"));

        store
            .clear()
            .expect("clearing with nothing on disk at all is not an error");
    }

    #[test]
    fn failed_write_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        // Force the final `fs::rename` to fail: a file cannot be renamed onto an existing
        // directory.
        fs::create_dir(&path).expect("create a directory at the target path");
        let store = FileStore::new(&path);

        store
            .save(&Session::from_token("tok"))
            .expect_err("rename onto an existing directory must fail");

        let leftovers: Vec<OsString> = fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(
            leftovers,
            vec![OsString::from("cookies.json")],
            "a failed write must not leave any temporary file behind, got {leftovers:?}"
        );
    }

    // ===================== Debug redaction =====================

    #[test]
    fn debug_only_ever_prints_the_path() {
        // `FileStore` holds nothing but a path, so a derived `Debug` cannot leak a credential —
        // confirmed here rather than merely asserted in a doc comment.
        let store = FileStore::new("/tmp/example/cookies.json");
        let debug = format!("{store:?}");
        assert!(debug.contains("cookies.json"));
    }

    /// Small helper so every test above reads as "write this exact byte string", without
    /// repeating the `expect` boilerplate.
    fn fs_write(path: &std::path::Path, contents: &str) {
        std::fs::write(path, contents).expect("write succeeds");
    }
}
