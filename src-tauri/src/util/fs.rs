//! Filesystem helpers shared across commands.
//!
//! - [`atomic_write`] — temp file + rename + fsync(parent) for crash-safe writes.
//! - [`read_capped`] — bounded read that errors (rather than truncating) on oversize.
//!
//! Both helpers are the canonical chokepoints called out in the Phase 12
//! security review (memory-bank/scans/phase12-security-review.md
//! §Cross-cutting concerns "Single-helper chokepoints"). Future commands
//! that write JSON / binary blobs into `app_data_dir` should reach for
//! these rather than re-implementing the pattern.

use std::path::Path;

use crate::error::AppError;

/// Write `bytes` atomically to `final_path`.
///
/// Pattern:
///   1. write to `<final_path>.tmp`
///   2. fsync the temp file (so the bytes are durable)
///   3. rename `<final_path>.tmp` -> `final_path` (atomic on the same volume)
///   4. fsync the parent directory (so the rename itself is durable)
///
/// A crash at any point leaves either the prior `final_path` (if it
/// existed) or no `final_path` at all — never a torn write. The temp
/// file lives next to `final_path` so the rename stays on the same
/// volume; cross-device renames would fall back to copy+unlink and lose
/// atomicity.
///
/// If `final_path`'s parent directory doesn't exist, this returns
/// `AppError::Io` rather than creating it — callers should `mkdir_p`
/// explicitly so the parent's permissions are intentional.
pub async fn atomic_write(final_path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = final_path.parent().ok_or_else(|| AppError::Io {
        message: format!(
            "atomic_write: {} has no parent directory",
            final_path.display()
        ),
    })?;

    // Build a sibling temp path: `<final>.tmp`. Same volume → rename is
    // atomic. The `.tmp` suffix is convention-only; nothing relies on
    // parsing it.
    let mut tmp_name = final_path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp_path = std::path::PathBuf::from(tmp_name);

    // Write + fsync the data file. Use `tokio::fs::File` so we can call
    // `sync_all` (the bare `write` helper doesn't fsync).
    let staged = {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| AppError::Io {
                message: format!("create {}: {}", tmp_path.display(), e),
            })?;
        async {
            file.write_all(bytes).await.map_err(|e| AppError::Io {
                message: format!("write {}: {}", tmp_path.display(), e),
            })?;
            // `write_all` only hands bytes to the buffered file; on a
            // filesystem that runs out of space the short write is reported by
            // the FLUSH, not by `write_all` or `sync_all`. Without this, a full
            // disk produced `Ok(())` over a truncated file — the ledger could
            // be silently destroyed while the install reported success.
            file.flush().await.map_err(|e| AppError::Io {
                message: format!("flush {}: {}", tmp_path.display(), e),
            })?;
            file.sync_all().await.map_err(|e| AppError::Io {
                message: format!("fsync {}: {}", tmp_path.display(), e),
            })?;
            Ok::<(), AppError>(())
        }
        .await
    };
    if let Err(error) = staged {
        // Never leave a partial `<final>.tmp` behind: it is invisible to the
        // app and accumulates across failures.
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(error);
    }

    // Atomic rename. On failure, remove the temp file: a leftover `<final>.tmp`
    // is invisible to the app and accumulates.
    if let Err(e) = tokio::fs::rename(&tmp_path, final_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(AppError::Io {
            message: format!(
                "rename {} -> {}: {}",
                tmp_path.display(),
                final_path.display(),
                e
            ),
        });
    }

    // fsync the parent directory so the rename itself is durable.
    // Opening a dir + sync_all is the POSIX-portable way; on macOS this
    // resolves to fsync(2) on a directory fd, which is supported.
    if let Ok(dir) = tokio::fs::File::open(parent).await {
        let _ = dir.sync_all().await;
        // Best-effort — some filesystems / runtimes refuse fsync on
        // directories; the data file's own fsync is the load-bearing
        // durability guarantee.
    }

    Ok(())
}

/// Read at most `max_bytes` from `path`. Returns `AppError::Io` if the
/// file's length exceeds the cap (does NOT silently truncate).
///
/// Use this for any read of an attacker-influenced or user-data file
/// where a deliberately oversize payload would cause OOM. The size check
/// uses `metadata().len()` so the cap is enforced *before* allocation.
pub async fn read_capped(path: &Path, max_bytes: u64) -> Result<Vec<u8>, AppError> {
    let meta = tokio::fs::metadata(path).await.map_err(|e| AppError::Io {
        message: format!("stat {}: {}", path.display(), e),
    })?;
    let size = meta.len();
    if size > max_bytes {
        return Err(AppError::Io {
            message: format!(
                "{} is {} bytes, exceeds cap of {}",
                path.display(),
                size,
                max_bytes
            ),
        });
    }
    tokio::fs::read(path).await.map_err(|e| AppError::Io {
        message: format!("read {}: {}", path.display(), e),
    })
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn atomic_write_creates_temp_then_renames() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let final_path = tmp.path().join("out.bin");
        let payload = b"hello world\nthis is a test\n";

        atomic_write(&final_path, payload)
            .await
            .expect("atomic_write");

        // Final path exists with the right content.
        let read_back = tokio::fs::read(&final_path).await.expect("read final");
        assert_eq!(read_back, payload);

        // Temp file no longer exists — the rename consumed it.
        let temp_sibling = tmp.path().join("out.bin.tmp");
        assert!(
            !temp_sibling.exists(),
            "temp file {} should not survive rename",
            temp_sibling.display()
        );
    }

    #[tokio::test]
    async fn atomic_write_overwrites_existing_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let final_path = tmp.path().join("out.bin");

        // Pre-existing payload.
        tokio::fs::write(&final_path, b"old").await.expect("seed");
        atomic_write(&final_path, b"new payload bytes")
            .await
            .expect("atomic_write");

        let read_back = tokio::fs::read(&final_path).await.expect("read");
        assert_eq!(read_back, b"new payload bytes");
    }

    #[tokio::test]
    async fn atomic_write_fails_when_parent_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Parent dir does not exist — we don't `mkdir_p` for callers.
        let final_path = tmp.path().join("missing").join("out.bin");
        let r = atomic_write(&final_path, b"x").await;
        assert!(r.is_err(), "expected error when parent dir is missing");
    }

    #[tokio::test]
    async fn atomic_write_cleans_up_the_temp_file_when_the_rename_fails() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // A non-empty directory at the final path makes `rename` fail.
        let final_path = tmp.path().join("out.bin");
        std::fs::create_dir_all(final_path.join("occupied")).expect("occupy the final path");

        atomic_write(&final_path, b"payload")
            .await
            .expect_err("renaming onto a non-empty directory must fail");

        assert!(
            !tmp.path().join("out.bin.tmp").exists(),
            "a failed write must not leave its temp file behind"
        );
    }

    #[tokio::test]
    async fn read_capped_returns_full_contents_within_cap() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("small.bin");
        let payload = b"short payload";
        tokio::fs::write(&path, payload).await.expect("write");

        let out = read_capped(&path, 1024).await.expect("read_capped");
        assert_eq!(out, payload);
    }

    #[tokio::test]
    async fn read_capped_rejects_oversize() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("big.bin");
        // Write 256 bytes; cap at 100. Must error, not truncate.
        let payload = vec![0xABu8; 256];
        tokio::fs::write(&path, &payload).await.expect("write");

        let r = read_capped(&path, 100).await;
        match r {
            Err(AppError::Io { message }) => {
                assert!(
                    message.contains("exceeds cap"),
                    "expected cap-exceeded message, got {:?}",
                    message
                );
            }
            other => panic!("expected Io error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn read_capped_accepts_exactly_at_cap() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("edge.bin");
        let payload = vec![0xCDu8; 64];
        tokio::fs::write(&path, &payload).await.expect("write");

        let out = read_capped(&path, 64).await.expect("read_capped");
        assert_eq!(out, payload);
    }

    #[tokio::test]
    async fn read_capped_errors_on_missing_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("does-not-exist.bin");
        assert!(read_capped(&path, 1024).await.is_err());
    }
}
