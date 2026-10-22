//! Filesystem credential store.
//!
//! Ported from `FileStorage` (`eero-api`'s `src/eero/api/auth_storage.py:425-606` at `v8.4.1`),
//! with one deliberate improvement over the Python original: atomic, mode-on-create file
//! permissions instead of a write-then-`chmod` sequence.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::auth::Session;
use crate::error::StorageError;

use super::CredentialStore;

/// The backend name carried by this module's [`StorageError::NotRetained`].
const BACKEND_NAME: &str = "file";

/// A [`CredentialStore`] backed by a single JSON file on disk.
///
/// Mirrors `FileStorage` (`auth_storage.py:425-606`): the file holds exactly the JSON object
/// `Session::to_json`/[`Session::from_json`] produce/consume (`session_id`, `schema_version`,
/// plus the legacy `user_token` read alias) — the same shape a Python `eero-api` install reads
/// and writes, so the two libraries can share one cookie file. There is deliberately no default
/// path: like `FileStorage.__init__`, which always requires an explicit `file_path`, choosing
/// *where* the file lives is left entirely to the caller (in `eero-api`'s ecosystem, that
/// choice belongs to the CLI layer, not this library).
///
/// # Verified saves
///
/// [`FileStore::save`] reads the file back after the atomic rename and parses it like a load
/// would; it returns `Ok(())` only if the stored token is the one that was saved. If the file is
/// missing, unparseable or holds another token (the destination was swapped or overwritten
/// between the rename and the read-back), it logs a fixed `WARN` — `Credential file did not
/// retain the saved session`, never the token — and returns [`StorageError::NotRetained`]
/// (`auth_storage.py:582-586`).
///
/// # Concurrency
///
/// Every `load`, `save` and `clear` on one `FileStore` runs under an internal lock held for the
/// whole operation (the read-back and a legacy-record migration included), so a `save` and a
/// `clear` cannot interleave: call order decides the outcome, and a `clear` can no longer remove a
/// concurrent `save`'s temporary file. Two `FileStore` values on the same path are *not*
/// coordinated with each other, exactly as in `eero-api` (`auth_storage.py:190-235`).
///
/// # Security: atomic `0600` creation, not `chmod` after the fact
///
/// Python's `FileStorage.save()` creates the temporary file with `tempfile.mkstemp` (mode `0600`)
/// and re-asserts `0600` with a trailing `os.chmod()` as defense in depth
/// (`auth_storage.py:555-569`). This port needs no such second step: [`FileStore::save`] writes
/// to a temporary file in the same directory, created with `0600` permissions *atomically* as
/// part of the file-creation syscall (via [`std::os::unix::fs::OpenOptionsExt::mode`]), then
/// renames it into place. A crash between the write and the rename leaves either the old file or
/// nothing at all — never a partially-written or loosely-permissioned target file.
///
/// **This guarantee is Unix-only.** `OpenOptionsExt::mode` has no equivalent in the Windows
/// standard library; on non-Unix platforms the temporary file is created with whatever default
/// permissions/ACL the OS applies, and this crate does not attempt to further restrict them.
/// Callers on Windows should not rely on the stored file being owner-only — treat the containing
/// directory's own ACLs as the real access boundary there.
///
/// # Security: writing through a symlink at the final path
///
/// Python's `FileStorage.save()` refuses to write when the *final* path is a symlink
/// (`os.path.islink` check before opening the temp file, `auth_storage.py:548-551`) — a defence
/// against a symlink planted at the cookie-file path being used to redirect a credential write
/// onto an attacker-chosen target. This port does not reproduce that check as a separate
/// precondition, and does not need to: the final step of [`FileStore::save`] is [`fs::rename`],
/// not an in-place open of `path`. POSIX `rename(2)` (and its Windows equivalent) *replaces*
/// whatever is at the destination — including a symlink — rather than following it, so the write
/// always lands at `path` itself, never at whatever a symlink there might point to. This is
/// verified by `save_replaces_a_symlink_at_the_destination_rather_than_following_it` below rather
/// than merely asserted in this comment.
///
/// # Security: no orphaned plaintext credential
///
/// The temporary file above is given a name that is unique for the lifetime of this process
/// (see `unique_temp_path`), and every error path in the create/write/sync/rename sequence
/// removes it before returning (see `write_private_atomically`) — a crash *between* those
/// steps is the only way a temp file can survive, and [`FileStore::clear`] proactively removes
/// any such orphan for `path` (see `remove_orphaned_temp_files`), so a stale, fully-valid
/// plaintext credential can never outlive an explicit `clear()` call.
pub struct FileStore {
    path: PathBuf,
    /// Serialises every operation on this instance; see the type's "Concurrency" docs. Guards no
    /// data, so a panic while it is held does not poison anything that matters.
    op_lock: Mutex<()>,
}

impl std::fmt::Debug for FileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl FileStore {
    /// Creates a store backed by `path`, which need not exist yet.
    ///
    /// Unlike `FileStorage.__init__` (`auth_storage.py:432-440`), which eagerly expands `~` and
    /// absolutizes the path at construction time, `path` is stored exactly as given — this port
    /// has no path-expansion dependency available and resolving `~`/relative paths is left to
    /// the caller, consistent with there being no default path anywhere in this crate.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            op_lock: Mutex::new(()),
        }
    }

    /// The path this store reads from and writes to.
    ///
    /// Mirrors the read-only `file_path` property (`auth_storage.py:442-444`).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Takes this instance's operation lock. A poisoned lock (a panic while it was held) is
    /// recovered: it guards no data, and a later operation must still be able to run.
    fn lock_operations(&self) -> MutexGuard<'_, ()> {
        self.op_lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The body of [`CredentialStore::load`]; the caller holds the operation lock.
    fn load_unlocked(&self) -> Result<Session, StorageError> {
        match fs::read_to_string(&self.path) {
            // Mirrors the "file does not exist" branch (`auth_storage.py:464-466`): this is the
            // expected "never logged in" state, not an error.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Session::empty()),
            Err(err) => Err(StorageError::Io(err)),
            Ok(contents) => {
                // A credential file with any group/other permission bit set (e.g. one this
                // crate did not itself create — hand-placed, copied in from a backup, or
                // written by an older/foreign tool) is readable by more than its owner. Never
                // fails the read — this is a diagnostic, not an enforced precondition — but it
                // must not go unnoticed either.
                warn_if_permissions_are_loose(&self.path);
                let (session, migrated) = Session::from_json_migrating(&contents)?;
                if migrated {
                    // Best-effort: never fails this read, see `migrate_and_verify`'s own docs.
                    // The unlocked save keeps the migration inside this operation's lock.
                    super::migrate_and_verify(&session, BACKEND_NAME, |migrated| {
                        self.save_unlocked(migrated)
                    });
                }
                Ok(session)
            }
        }
    }

    /// The body of [`CredentialStore::save`]; the caller holds the operation lock.
    fn save_unlocked(&self, session: &Session) -> Result<(), StorageError> {
        // Mirrors `os.makedirs(cookie_dir, exist_ok=True)` (`auth_storage.py:545-546`); an empty
        // parent (a bare file name with no directory component) has nothing to create.
        //
        // Every directory this call creates is created with owner-only (`0700`) permissions from
        // the moment it is created, on Unix — matching the atomic 0600-on-create discipline
        // `create_private_file` already applies to the file itself, rather than relying on the
        // process umask (which a caller could have widened, e.g. `umask 022`, leaving a
        // group/other-readable directory the credential file then inherits its own looser
        // default permissions from on some platforms/filesystems).
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            create_private_dir_all(parent)?;
        }
        let json = session.to_json()?;
        write_private_atomically(&self.path, json.as_bytes())?;
        verify_file_retains(&self.path, session)?;
        tracing::debug!("Saved authentication data to {}", self.path.display());
        Ok(())
    }

    /// The body of [`CredentialStore::clear`]; the caller holds the operation lock.
    fn clear_unlocked(&self) -> Result<(), StorageError> {
        let result = match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            // Mirrors the tolerant `os.path.exists(...)` guard before `os.remove`
            // (`auth_storage.py:601-604`): removing an entry that was never there is not an
            // error.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(StorageError::Io(err)),
        };
        // Also remove any temporary file `write_private_atomically` may have left behind after a
        // crash between creating it and renaming it into place — otherwise a full plaintext
        // credential can survive at a predictable path even after `clear()` reports success.
        // Best-effort: a missing (or already-removed) orphan is not an error, matching the
        // tolerance above. Held under the operation lock, so it can never delete the in-flight
        // temporary file of a concurrent `save` on this instance.
        remove_orphaned_temp_files(&self.path);
        result
    }
}

impl CredentialStore for FileStore {
    fn load(&self) -> Result<Session, StorageError> {
        let _guard = self.lock_operations();
        self.load_unlocked()
    }

    fn save(&self, session: &Session) -> Result<(), StorageError> {
        let _guard = self.lock_operations();
        self.save_unlocked(session)
    }

    fn clear(&self) -> Result<(), StorageError> {
        let _guard = self.lock_operations();
        self.clear_unlocked()
    }
}

/// Reads `path` back and checks it holds `saved`'s token, like a load would parse it.
///
/// Mirrors the read-back at the end of `FileStorage._save_sync` (`auth_storage.py:582-586`). A
/// file that is missing, no longer parses, or carries a different token was not retained: that
/// logs the fixed `WARN` below (never the token) and returns [`StorageError::NotRetained`]. Any
/// other read failure is an I/O error and is returned as such.
fn verify_file_retains(path: &Path, saved: &Session) -> Result<(), StorageError> {
    let retained = match fs::read_to_string(path) {
        // No migration and no loose-permission warning: this is the file this save just wrote.
        Ok(contents) => Session::from_json(&contents)
            .is_ok_and(|stored| stored.expose_token() == saved.expose_token()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => return Err(StorageError::Io(err)),
    };
    if retained {
        Ok(())
    } else {
        tracing::warn!("Credential file did not retain the saved session");
        Err(StorageError::NotRetained {
            backend: BACKEND_NAME.to_owned(),
        })
    }
}

/// Writes `contents` to `path`, creating or replacing it atomically with owner-only (`0600`)
/// permissions from the moment the file is created.
///
/// Writes to a sibling temporary file first (same directory as `path`, so the final
/// [`fs::rename`] is guaranteed to be on the same filesystem and therefore atomic), then renames
/// it over `path`.
///
/// # Security: no orphaned plaintext after a failed write
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
/// # Security: unique temporary names
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

/// Creates `parent` and every missing ancestor directory with owner-only (`0700`) permissions,
/// set at creation time rather than via a separate `chmod` afterwards — the same atomic-mode
/// discipline [`create_private_file`] applies to the credential file itself (the directory-permission hardening).
///
/// [`std::fs::DirBuilder::mode`] applies to every directory this call actually creates, not just
/// the deepest one; an already-existing ancestor is left with whatever permissions it already
/// has (matching `create_dir_all`'s own "existing directories are not modified" contract).
#[cfg(unix)]
fn create_private_dir_all(parent: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
}

/// Non-Unix fallback: creates `parent` and every missing ancestor with the platform's default
/// permissions. See [`FileStore`]'s own doc comment for what this means for callers on Windows.
#[cfg(not(unix))]
fn create_private_dir_all(parent: &Path) -> std::io::Result<()> {
    fs::create_dir_all(parent)
}

/// Returns `true` if `path`'s permission bits grant any access to group or other (i.e. anything
/// beyond owner-only `0600`/`0700`), `false` if `path` cannot be inspected at all (never treated
/// as loose — there is nothing this crate can usefully warn about for a file it cannot stat).
///
/// Unix-only: there is no portable notion of "group/other" permission bits to inspect elsewhere;
/// see [`FileStore`]'s own doc comment for what that means for callers on Windows.
#[cfg(unix)]
fn has_loose_permissions(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o077 != 0)
}

/// Logs a fixed `WARN` (never fails, never blocks the read) if [`has_loose_permissions`] reports
/// `path` is readable/writable by group or other — e.g. a credential file this crate did not
/// itself create, copied in from elsewhere, or left over from an older non-atomic
/// implementation. This crate's own [`create_private_file`]/
/// [`create_private_dir_all`] never produce such a file, but nothing prevents one from appearing
/// at `path` by some other means, and a loosely-permissioned plaintext credential file should
/// never go unnoticed merely because it still parses.
#[cfg(unix)]
fn warn_if_permissions_are_loose(path: &Path) {
    if has_loose_permissions(path) {
        tracing::warn!(
            path = %path.display(),
            "credential file has group/other-accessible permissions; expected owner-only (0600)"
        );
    }
}

/// Non-Unix no-op: there is no portable permission-bit check to perform. See
/// [`has_loose_permissions`]'s own doc comment.
#[cfg(not(unix))]
fn warn_if_permissions_are_loose(_path: &Path) {}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use tracing::Level;

    use super::{BACKEND_NAME, CredentialStore, FileStore, verify_file_retains};
    use crate::auth::Session;
    use crate::error::StorageError;
    use crate::storage::test_support::capture_logs;
    use crate::storage::{load_async, save_async};

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
    fn round_trip_through_v8_0_4_cookies_json_shape() {
        // The exact on-disk shape `FileStorage.save()` writes at v8.0.4 (`auth_storage.py:50-56`):
        // key order `session_id`, `schema_version`, with Python's default `json.dump` separators
        // (a space after `:`/`,`). This differs byte-for-byte from this port's compact
        // `serde_json` output, but both are valid JSON over the same wire contract, so a file
        // a Python install wrote must still load cleanly here.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let python_written = r#"{"session_id": "session_valid_id", "schema_version": 2}"#;
        fs_write(&path, python_written);

        let store = FileStore::new(&path);
        let session = store.load().expect("load succeeds");

        assert_eq!(session.expose_token(), "session_valid_id");

        // Re-saving through this port must remain a valid credential file: reloading it here
        // must still recover the token.
        store.save(&session).expect("save succeeds");
        let reloaded = store.load().expect("load succeeds");
        assert_eq!(reloaded.expose_token(), "session_valid_id");
    }

    // ===================== legacy migration (no schema_version) =====================

    #[test]
    fn load_migrates_a_legacy_record_and_the_migration_read_back_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        // A pre-v8.0.4 record: no `schema_version`, carries now-dropped legacy fields.
        fs_write(
            &path,
            r#"{"session_id":"legacy-token","refresh_token":"rt-1","session_expiry":"2099-01-01T00:00:00"}"#,
        );
        let store = FileStore::new(&path);

        let session = store
            .load()
            .expect("load succeeds, legacy record tolerated");
        assert_eq!(session.expose_token(), "legacy-token");

        // The migration re-save must have overwritten the file with the current schema shape.
        let on_disk = fs::read_to_string(&path).expect("file still exists");
        assert!(on_disk.contains("\"schema_version\":2"));
        assert!(!on_disk.contains("refresh_token"));
        assert!(!on_disk.contains("session_expiry"));

        // And a second load must not report a migration a second time.
        let reloaded = store.load().expect("second load succeeds");
        assert_eq!(reloaded.expose_token(), "legacy-token");
    }

    #[test]
    fn load_of_a_current_schema_record_is_not_treated_as_a_migration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        fs_write(&path, r#"{"session_id":"tok","schema_version":2}"#);
        let store = FileStore::new(&path);

        let before = fs::read_to_string(&path).expect("file exists");
        let session = store.load().expect("load succeeds");
        assert_eq!(session.expose_token(), "tok");
        let after = fs::read_to_string(&path).expect("file still exists");
        assert_eq!(
            before, after,
            "a current-schema record must not be rewritten"
        );
    }

    // ===================== symlink-at-destination (v8.0.4 parity note) =====================

    #[cfg(unix)]
    #[test]
    fn save_replaces_a_symlink_at_the_destination_rather_than_following_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real_target = dir.path().join("attacker-owned-target.json");
        fs_write(&real_target, "should never be written to");
        let cookie_path = dir.path().join("cookies.json");
        std::os::unix::fs::symlink(&real_target, &cookie_path).expect("symlink created");

        let store = FileStore::new(&cookie_path);
        store
            .save(&Session::from_token("tok"))
            .expect("save succeeds even though the destination is a symlink");

        // The symlink itself was replaced by a regular file; the target it used to point to was
        // never touched.
        assert!(
            !cookie_path.is_symlink(),
            "save must replace the symlink, not write through it"
        );
        let target_contents =
            fs::read_to_string(&real_target).expect("the original target file still exists");
        assert_eq!(target_contents, "should never be written to");
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

    // ===================== Parent directory / loose-permission warning =====================

    #[cfg(unix)]
    #[test]
    fn save_creates_every_missing_parent_directory_with_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("dirs").join("cookies.json");
        let store = FileStore::new(&path);

        store
            .save(&Session::from_token("tok"))
            .expect("save succeeds");

        for created in [dir.path().join("nested"), dir.path().join("nested/dirs")] {
            let mode = fs::metadata(&created)
                .expect("directory exists")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                mode, 0o700,
                "expected {created:?} to be owner-only, got {mode:o}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn has_loose_permissions_is_false_for_a_freshly_saved_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let store = FileStore::new(&path);
        store
            .save(&Session::from_token("tok"))
            .expect("save succeeds");

        assert!(!super::has_loose_permissions(&path));
    }

    #[cfg(unix)]
    #[test]
    fn has_loose_permissions_is_true_for_a_world_readable_file_and_load_still_succeeds() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        fs_write(&path, r#"{"session_id":"tok","schema_version":2}"#);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("chmod succeeds");

        assert!(super::has_loose_permissions(&path));

        let store = FileStore::new(&path);
        let session = store.load().expect(
            "a loosely-permissioned file must still load — this is a warning, not a failure",
        );
        assert_eq!(session.expose_token(), "tok");
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

    // ===================== clear(): orphaned temporary files =====================

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

    // ===================== read-back verification =====================

    #[test]
    fn a_successful_save_is_ok_and_logs_no_warning() {
        let (_guard, logs) = capture_logs();
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileStore::new(dir.path().join("cookies.json"));

        store
            .save(&Session::from_token("file_token"))
            .expect("save succeeds");

        assert!(logs.messages(Level::WARN).is_empty());
        assert_eq!(logs.count(Level::DEBUG, "Saved authentication data to"), 1);
        assert!(!logs.mentions("file_token"));
    }

    #[test]
    fn a_read_back_that_finds_another_token_is_not_retained() {
        let (_guard, logs) = capture_logs();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        fs_write(&path, r#"{"session_id":"other","schema_version":2}"#);

        let err = verify_file_retains(&path, &Session::from_token("file_token"))
            .expect_err("another token is on disk");

        assert!(
            matches!(&err, StorageError::NotRetained { backend } if backend == BACKEND_NAME),
            "got {err:?}"
        );
        assert_eq!(
            logs.messages(Level::WARN),
            vec!["Credential file did not retain the saved session".to_owned()]
        );
        assert!(!logs.mentions("file_token") && !logs.mentions("other"));
        assert!(!err.to_string().contains("file_token"));
    }

    #[test]
    fn a_read_back_that_finds_the_same_token_is_retained_even_for_a_legacy_record() {
        let (_guard, logs) = capture_logs();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");

        fs_write(&path, r#"{"session_id":"file_token","schema_version":2}"#);
        verify_file_retains(&path, &Session::from_token("file_token")).expect("same token");

        fs_write(&path, r#"{"user_token":"file_token"}"#);
        verify_file_retains(&path, &Session::from_token("file_token"))
            .expect("a legacy record is parsed like a load would parse it");

        assert!(logs.messages(Level::WARN).is_empty());
    }

    #[test]
    fn a_missing_or_unparseable_file_is_not_retained() {
        let (_guard, logs) = capture_logs();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let saved = Session::from_token("file_token");

        let missing = verify_file_retains(&path, &saved).expect_err("nothing on disk");
        assert!(matches!(missing, StorageError::NotRetained { .. }));

        fs_write(&path, "not json");
        let garbled = verify_file_retains(&path, &saved).expect_err("not a record");
        assert!(matches!(garbled, StorageError::NotRetained { .. }));

        assert_eq!(
            logs.count(
                Level::WARN,
                "Credential file did not retain the saved session"
            ),
            2,
            "one fixed WARN per failure"
        );
    }

    #[test]
    fn a_read_back_that_fails_for_another_reason_is_an_io_error() {
        let (_guard, logs) = capture_logs();
        let dir = tempfile::tempdir().expect("tempdir");

        // Reading a directory as a file fails with something other than "not found".
        let err = verify_file_retains(dir.path(), &Session::from_token("file_token"))
            .expect_err("a directory cannot be read as the record");

        assert!(matches!(err, StorageError::Io(_)), "got {err:?}");
        assert!(logs.messages(Level::WARN).is_empty());
    }

    #[test]
    fn save_under_a_parent_that_is_a_regular_file_is_an_io_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let blocker = dir.path().join("not-a-directory");
        fs_write(&blocker, "blocking");
        let store = FileStore::new(blocker.join("cookies.json"));

        let err = store
            .save(&Session::from_token("tok"))
            .expect_err("the parent directory cannot be created");

        assert!(matches!(err, StorageError::Io(_)), "got {err:?}");
    }

    // ===================== legacy migration: logging and failure =====================

    #[test]
    fn a_legacy_load_logs_the_migration_and_its_verified_read_back() {
        let (_guard, logs) = capture_logs();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        fs_write(&path, r#"{"user_token":"legacy_token"}"#);
        // Owner-only, like a file this crate wrote, so the loose-permission warning stays quiet.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod");
        }

        let session = FileStore::new(&path).load().expect("load succeeds");

        assert_eq!(session.expose_token(), "legacy_token");
        assert_eq!(
            logs.count(
                Level::DEBUG,
                "Migrated legacy credential record in file storage"
            ),
            1
        );
        assert_eq!(
            logs.count(
                Level::DEBUG,
                "Migration read-back verified for file storage"
            ),
            1
        );
        assert!(logs.messages(Level::WARN).is_empty());
        assert!(!logs.mentions("legacy_token"));
    }

    #[cfg(unix)]
    #[test]
    fn a_migration_that_cannot_be_written_still_returns_the_session_with_a_warning() {
        use std::os::unix::fs::PermissionsExt;

        let (_guard, logs) = capture_logs();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        fs_write(&path, r#"{"user_token":"legacy_token"}"#);
        // A read-only directory refuses the temporary file the migration save needs.
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).expect("chmod");
        let restore = RestoreMode(dir.path().to_path_buf());
        if fs::File::create(dir.path().join("probe")).is_ok() {
            // Running with privileges that ignore directory modes (root): nothing to observe.
            return;
        }

        let session = FileStore::new(&path)
            .load()
            .expect("a failed migration write must not fail the load");

        assert_eq!(session.expose_token(), "legacy_token");
        assert_eq!(
            logs.count(
                Level::WARN,
                "Migration read-back did not match for file storage; in-memory credentials are \
                 still returned to the caller"
            ),
            1
        );
        assert!(!logs.mentions("legacy_token"));
        drop(restore);
    }

    /// Restores a directory's mode on drop so the tempdir can be removed.
    #[cfg(unix)]
    struct RestoreMode(std::path::PathBuf);

    #[cfg(unix)]
    impl Drop for RestoreMode {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
        }
    }

    // ===================== per-instance operation lock =====================

    /// Holds a [`FileStore`]'s operation lock on a thread of its own until released, standing in
    /// for an operation that is running inside the store.
    struct LockHolder {
        release: mpsc::Sender<()>,
        thread: std::thread::JoinHandle<()>,
    }

    impl LockHolder {
        fn hold(store: &Arc<FileStore>) -> Self {
            let (locked_tx, locked_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let store = Arc::clone(store);
            let thread = std::thread::spawn(move || {
                let _guard = store.lock_operations();
                locked_tx.send(()).expect("the test is waiting");
                let _ = release_rx.recv();
            });
            locked_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("the holder takes the lock");
            Self {
                release: release_tx,
                thread,
            }
        }

        fn release(self) {
            let _ = self.release.send(());
            self.thread.join().expect("the holder thread finishes");
        }
    }

    fn new_store(dir: &tempfile::TempDir) -> Arc<FileStore> {
        Arc::new(FileStore::new(dir.path().join("cookies.json")))
    }

    /// Runs `operation` on a thread of its own while the lock is held elsewhere, and checks it
    /// neither runs nor finishes until the lock is released.
    fn assert_waits_for_the_lock(operation: impl FnOnce(&FileStore) + Send + 'static) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = new_store(&dir);
        let holder = LockHolder::hold(&store);

        let (done_tx, done_rx) = mpsc::channel();
        let worker = Arc::clone(&store);
        let thread = std::thread::spawn(move || {
            operation(&worker);
            let _ = done_tx.send(());
        });
        assert!(
            done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "the operation ran while another one held the lock"
        );

        holder.release();
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the operation completes once the lock is free");
        thread.join().expect("the operation thread finishes");
    }

    #[test]
    fn load_waits_for_the_operation_lock() {
        assert_waits_for_the_lock(|store| {
            store.load().expect("load succeeds");
        });
    }

    #[test]
    fn save_waits_for_the_operation_lock() {
        assert_waits_for_the_lock(|store| {
            store
                .save(&Session::from_token("tok"))
                .expect("save succeeds");
        });
    }

    #[test]
    fn clear_waits_for_the_operation_lock() {
        assert_waits_for_the_lock(|store| {
            store.clear().expect("clear succeeds");
        });
    }

    #[test]
    fn operations_on_one_instance_are_serialised() {
        // Spelled as the issue is worded: a thread that wants the lock while it is taken must
        // not get past it, and does as soon as the holder lets go.
        let dir = tempfile::tempdir().expect("tempdir");
        let store = new_store(&dir);
        let guard = store.lock_operations();

        let (done_tx, done_rx) = mpsc::channel();
        let worker = Arc::clone(&store);
        let thread = std::thread::spawn(move || {
            worker.clear().expect("clear succeeds");
            let _ = done_tx.send(());
        });
        assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(guard);
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the clear completes once the guard is dropped");
        thread.join().expect("the clear thread finishes");
    }

    #[test]
    fn save_then_clear_ends_cleared() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = new_store(&dir);
        let path = store.path().to_path_buf();

        // A save is in flight: it has done its work and still holds the lock.
        let guard = store.lock_operations();
        store
            .save_unlocked(&Session::from_token("saved"))
            .expect("the in-flight save works");
        let worker = Arc::clone(&store);
        let clearer = std::thread::spawn(move || worker.clear().expect("clear succeeds"));
        std::thread::sleep(Duration::from_millis(50));
        assert!(path.exists(), "the queued clear waits behind the save");
        assert!(!clearer.is_finished());

        drop(guard);
        clearer.join().expect("the clear thread finishes");
        assert!(
            !path.exists(),
            "call order decides: save, then clear, ends cleared"
        );
    }

    #[test]
    fn clear_then_save_ends_saved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = new_store(&dir);
        let path = store.path().to_path_buf();
        store
            .save(&Session::from_token("old"))
            .expect("seed save succeeds");

        // A clear is in flight: it has done its work and still holds the lock.
        let guard = store.lock_operations();
        store.clear_unlocked().expect("the in-flight clear works");
        let worker = Arc::clone(&store);
        let saver = std::thread::spawn(move || {
            worker
                .save(&Session::from_token("new"))
                .expect("save succeeds");
        });
        std::thread::sleep(Duration::from_millis(50));
        assert!(!path.exists(), "the queued save waits behind the clear");
        assert!(!saver.is_finished());

        drop(guard);
        saver.join().expect("the save thread finishes");
        assert_eq!(
            store.load().expect("load succeeds").expose_token(),
            "new",
            "call order decides: clear, then save, ends saved"
        );
    }

    #[test]
    fn a_panic_while_the_lock_is_held_does_not_wedge_the_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = new_store(&dir);
        let poisoner = Arc::clone(&store);
        let result = std::thread::spawn(move || {
            let _guard = poisoner.lock_operations();
            panic!("a store operation panicked");
        })
        .join();
        assert!(result.is_err());

        store
            .save(&Session::from_token("tok"))
            .expect("a poisoned lock is recovered, not propagated");
        assert_eq!(store.load().expect("load").expose_token(), "tok");
    }

    // ===================== async adapters: waiting for the lock, never skipping =====================

    /// Reads the token straight from disk, without taking the operation lock.
    fn token_on_disk(path: &Path) -> Option<String> {
        let contents = fs::read_to_string(path).ok()?;
        let session = Session::from_json(&contents).ok()?;
        Some(session.expose_token().to_owned())
    }

    async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[tokio::test]
    async fn a_follow_up_async_load_waits_behind_the_running_operation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = new_store(&dir);
        store
            .save(&Session::from_token("old"))
            .expect("seed save succeeds");
        let holder = LockHolder::hold(&store);

        let load = tokio::spawn(load_async(Arc::clone(&store) as _));
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert!(
            !load.is_finished(),
            "a follow-up operation waits instead of racing the running one"
        );

        holder.release();
        let session = load
            .await
            .expect("the load task finishes")
            .expect("load succeeds");
        assert_eq!(session.expose_token(), "old");
    }

    #[tokio::test]
    async fn an_aborted_save_async_still_waiting_for_the_lock_still_lands() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = new_store(&dir);
        let path = store.path().to_path_buf();
        store
            .save(&Session::from_token("old"))
            .expect("seed save succeeds");
        let holder = LockHolder::hold(&store);

        let queued = tokio::spawn(save_async(
            Arc::clone(&store) as _,
            Session::from_token("queued"),
        ));
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        queued.abort();
        assert!(
            queued
                .await
                .expect_err("the awaiting task was aborted")
                .is_cancelled()
        );
        assert_eq!(token_on_disk(&path).as_deref(), Some("old"));

        // Dropping the future never skips the write: the blocking task was already handed to
        // the pool, so it runs as soon as the lock frees.
        holder.release();
        wait_until("the aborted save to land", || {
            token_on_disk(&path).as_deref() == Some("queued")
        })
        .await;
    }

    // ===================== Debug redaction =====================

    #[test]
    fn debug_only_ever_prints_the_path() {
        // `FileStore` holds a path and a lock and nothing else, so its `Debug` cannot leak a
        // credential — confirmed here rather than merely asserted in a doc comment.
        let store = FileStore::new("/tmp/example/cookies.json");
        let debug = format!("{store:?}");
        assert!(debug.contains("cookies.json"));
        assert!(debug.contains("FileStore"));
    }

    /// Small helper so every test above reads as "write this exact byte string", without
    /// repeating the `expect` boilerplate.
    fn fs_write(path: &std::path::Path, contents: &str) {
        std::fs::write(path, contents).expect("write succeeds");
    }
}
