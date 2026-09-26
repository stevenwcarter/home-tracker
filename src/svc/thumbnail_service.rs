//! Serves cached thumbnails and generates missing ones exactly once per key.

use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, PoisonError};
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use tokio::sync::{Mutex, Semaphore};
use tokio::task;

use crate::db::SqlitePool;
use crate::models::Thumbnail;
use crate::svc::blob::Blob;
use crate::svc::thumbnail::ThumbSize;
use crate::svc::{attachment, thumbnail};

/// One in-flight generation: `(sha256, size)`.
type Key = (String, ThumbSize);
type KeyLocks = StdMutex<HashMap<Key, Arc<Mutex<()>>>>;

/// Looks thumbnails up in the database and, on a miss, generates, stores and
/// returns them. Thumbnails are keyed by the original's content hash, so
/// every row sharing bytes shares them. Concurrent misses for the same key
/// wait for one generation instead of each decoding the original; different
/// keys never wait on each other. At most [`ThumbnailService::permits`]
/// generations run at once, so a burst of misses cannot hold every original
/// in memory or starve the blocking pool. An original that fails to decode
/// is remembered, so it is not decoded again on every request for it.
pub struct ThumbnailService {
    pool: SqlitePool,
    data_dir: PathBuf,
    locks: KeyLocks,
    /// The sha256 of every original that failed to decode, for the process
    /// lifetime. Never pruned: it is bounded by the distinct originals.
    failed: StdMutex<HashSet<String>>,
    generations: AtomicU64,
    permits: Semaphore,
    permit_count: usize,
}

impl ThumbnailService {
    pub fn new(pool: SqlitePool, data_dir: PathBuf) -> Arc<Self> {
        let permit_count = thread::available_parallelism().map_or(2, |n| n.get());
        Arc::new(Self {
            pool,
            data_dir,
            locks: StdMutex::new(HashMap::new()),
            failed: StdMutex::new(HashSet::new()),
            generations: AtomicU64::new(0),
            permits: Semaphore::new(permit_count),
            permit_count,
        })
    }

    /// The data directory originals are read from.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// How many generations may run concurrently: one per available core.
    pub fn permits(&self) -> usize {
        self.permit_count
    }

    /// How many thumbnails this service has generated (cache hits excluded).
    pub fn generations(&self) -> u64 {
        self.generations.load(Ordering::Relaxed)
    }

    /// `blob`'s thumbnail of exactly `size`, generating it on a miss. `None`
    /// when the blob is not thumbnailable or its original file is missing or
    /// does not decode; `Err` only for unexpected failures, whose messages
    /// name neither the hash (the original's file name) nor a filesystem
    /// path. Callers add which row the blob belongs to.
    pub async fn get_or_generate(&self, blob: &Blob, size: ThumbSize) -> Result<Option<Thumbnail>> {
        if !thumbnail::is_thumbnailable(&blob.mime_type) {
            return Ok(None);
        }
        // Stored thumbnails are looked up before the failure cache: an
        // undecodable original must not hide a size that is already stored.
        let db_size = i32::try_from(size.get()).context("thumbnail size out of range")?;
        if let Some(hit) = self.cached(&blob.sha256, db_size)? {
            return Ok(Some(hit));
        }
        if self.has_failed(&blob.sha256) {
            return Ok(None);
        }

        let lease = self.lease((blob.sha256.clone(), size));
        let _guard = lease.lock.lock().await;
        // Another request may have generated it, or failed to, while this one waited.
        if let Some(hit) = self.cached(&blob.sha256, db_size)? {
            return Ok(Some(hit));
        }
        if self.has_failed(&blob.sha256) {
            return Ok(None);
        }
        let Some(generated) = self.generate(&blob.sha256, size).await? else {
            return Ok(None);
        };
        let row = generated.into_row(blob.sha256.clone(), size, Utc::now().naive_utc())?;
        let mut conn = self
            .pool
            .get()
            .context("db connection for thumbnail insert")?;
        attachment::insert_thumbnail(&mut conn, &row)?;
        Ok(Some(row))
    }

    /// Decodes and resizes the original with `sha256` off the async runtime.
    /// `None` when the original file is missing or does not decode; the
    /// latter is remembered so it is not attempted again.
    async fn generate(
        &self,
        sha256: &str,
        size: ThumbSize,
    ) -> Result<Option<thumbnail::Generated>> {
        // Held across the read too, so waiting requests do not each buffer an original.
        let _permit = self
            .permits
            .acquire()
            .await
            .context("thumbnail semaphore closed")?;
        let path = attachment::original_path(&self.data_dir, sha256);
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                tracing::warn!(%sha256, path = %path.display(), "original file missing");
                return Ok(None);
            }
            Err(err) => {
                // The path goes to the log only: error messages reach clients.
                tracing::error!(%sha256, path = %path.display(), %err, "could not read original");
                return Err(anyhow!(err).context("reading an original"));
            }
        };
        let started = Instant::now();
        let px = size.get();
        let decoded = task::spawn_blocking(move || thumbnail::generate_bytes(&bytes, px))
            .await
            .context("thumbnail task panicked")?;
        let generated = match decoded {
            Ok(generated) => generated,
            Err(err) => {
                tracing::warn!(%sha256, "original could not be decoded; not retrying: {err:#}");
                self.failed
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(sha256.to_owned());
                return Ok(None);
            }
        };
        self.generations.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            %sha256,
            size = px,
            elapsed_ms = started.elapsed().as_millis(),
            "generated thumbnail"
        );
        Ok(Some(generated))
    }

    /// Whether the original with `sha256` already failed to decode.
    fn has_failed(&self, sha256: &str) -> bool {
        self.failed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(sha256)
    }

    /// The per-key lock for `key`, created on first use.
    fn lease(&self, key: Key) -> KeyLease<'_> {
        let mut locks = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
        let lock = Arc::clone(locks.entry(key.clone()).or_default());
        KeyLease {
            locks: &self.locks,
            key,
            lock,
        }
    }

    fn cached(&self, sha256: &str, size: i32) -> Result<Option<Thumbnail>> {
        let mut conn = self.pool.get().context("db connection")?;
        attachment::thumbnail(&mut conn, sha256, size)
    }
}

/// A claim on one key's lock. The last lease dropped removes the map entry,
/// so the map only holds keys with requests in flight — even when a request
/// is cancelled mid-generation.
struct KeyLease<'a> {
    locks: &'a KeyLocks,
    key: Key,
    lock: Arc<Mutex<()>>,
}

impl Drop for KeyLease<'_> {
    fn drop(&mut self) {
        let mut locks = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
        // Leases clone the `Arc` only under the map lock, so with the lock held
        // a count of two (the map and this lease) means no one else is waiting.
        if Arc::strong_count(&self.lock) == 2 {
            locks.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use tokio::time;

    use super::*;
    use crate::db::TestDb;
    use crate::svc::fixtures::jpeg;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// A JPEG blob with content hash `sha256`.
    fn blob(sha256: &str) -> Blob {
        Blob {
            sha256: sha256.to_owned(),
            mime_type: "image/jpeg".to_owned(),
        }
    }

    /// A service whose data dir holds [`SHA`], a 64×48 JPEG. No attachment
    /// row points at it: the service needs only the blob.
    fn service() -> (Arc<ThumbnailService>, TestDb, tempfile::TempDir) {
        let db = TestDb::new();
        let data = tempfile::tempdir().unwrap();
        let path = attachment::original_path(data.path(), SHA);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, jpeg(64, 48)).unwrap();
        let svc = ThumbnailService::new(db.pool.clone(), data.path().to_path_buf());
        (svc, db, data)
    }

    /// Writes an original with `sha256` that has JPEG magic but does not decode.
    fn write_undecodable(data: &tempfile::TempDir, sha256: &str) {
        fs::write(
            attachment::original_path(data.path(), sha256),
            b"\xFF\xD8\xFF junk",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn a_held_key_blocks_only_that_key() {
        // Review focus 1: different sizes must not serialize behind each other.
        let (svc, _db, _data) = service();
        let [small, medium] = [300, 500].map(|px| thumbnail::allowed_size(px).unwrap());
        let held = svc.lease((SHA.to_owned(), small));
        let _guard = held.lock.lock().await;
        let photo = blob(SHA);

        let other = time::timeout(Duration::from_secs(10), svc.get_or_generate(&photo, medium))
            .await
            .expect("a different size must not wait for the held key")
            .unwrap();
        assert_eq!(other.unwrap().size, 500);

        let same = time::timeout(
            Duration::from_millis(200),
            svc.get_or_generate(&photo, small),
        );
        assert!(
            same.await.is_err(),
            "the held key must make its waiters wait"
        );
        assert_eq!(svc.generations(), 1);
    }

    #[tokio::test]
    async fn the_lock_map_is_emptied_after_each_request() {
        let (svc, _db, _data) = service();
        let size = thumbnail::allowed_size(300).unwrap();
        svc.get_or_generate(&blob(SHA), size)
            .await
            .unwrap()
            .unwrap();
        // A missing original is the other early-return path.
        assert!(
            svc.get_or_generate(&blob(&"ab".repeat(32)), size)
                .await
                .unwrap()
                .is_none()
        );
        assert!(svc.locks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_failure_cache_is_keyed_by_content() {
        let (svc, _db, data) = service();
        let size = thumbnail::allowed_size(300).unwrap();
        let bad = "ee".repeat(32);
        write_undecodable(&data, &bad);

        assert!(
            svc.get_or_generate(&blob(&bad), size)
                .await
                .unwrap()
                .is_none()
        );
        assert!(svc.failed.lock().unwrap().contains(&bad));

        // Other bytes, say a re-import's, are not held back by the failure.
        let thumb = svc
            .get_or_generate(&blob(SHA), size)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(thumb.size, 300);
        assert_eq!(svc.generations(), 1);
    }

    #[tokio::test]
    async fn a_failed_decode_does_not_hide_stored_thumbnails() {
        // An imported 500px thumbnail stays servable after its original fails
        // to decode for another size.
        let (svc, db, data) = service();
        let bad = "ee".repeat(32);
        write_undecodable(&data, &bad);
        attachment::insert_thumbnail(
            &mut db.pool.get().unwrap(),
            &Thumbnail {
                sha256: bad.clone(),
                size: 500,
                mime_type: thumbnail::GENERATED_MIME.to_owned(),
                width: 4,
                height: 3,
                data: vec![1; 8],
                created_at: Utc::now().naive_utc(),
            },
        )
        .unwrap();
        let [small, medium] = [300, 500].map(|px| thumbnail::allowed_size(px).unwrap());

        assert!(
            svc.get_or_generate(&blob(&bad), small)
                .await
                .unwrap()
                .is_none()
        );
        let stored = svc
            .get_or_generate(&blob(&bad), medium)
            .await
            .unwrap()
            .expect("the stored 500px thumbnail is still served");
        assert_eq!(stored.data, [1; 8]);
    }

    #[tokio::test]
    async fn a_blob_that_is_not_thumbnailable_is_none_without_decoding() {
        let (svc, _db, _data) = service();
        let pdf = Blob {
            mime_type: "application/pdf".to_owned(),
            ..blob(SHA)
        };
        let size = thumbnail::allowed_size(300).unwrap();
        assert!(svc.get_or_generate(&pdf, size).await.unwrap().is_none());
        assert_eq!(svc.generations(), 0);
    }
}
