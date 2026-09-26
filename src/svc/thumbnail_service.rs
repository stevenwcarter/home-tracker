//! Serves cached thumbnails and generates missing ones exactly once per key.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, PoisonError};
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::Mutex;
use tokio::task;

use crate::db::SqlitePool;
use crate::models::{Attachment, Thumbnail};
use crate::svc::{attachment, thumbnail};

/// The MIME type of every thumbnail this service generates.
const GENERATED_MIME: &str = "image/webp";

/// One in-flight generation: `(attachment_id, size)`.
type Key = (String, u32);
type KeyLocks = StdMutex<HashMap<Key, Arc<Mutex<()>>>>;

/// Looks thumbnails up in the database and, on a miss, generates, stores and
/// returns them. Concurrent misses for the same key wait for one generation
/// instead of each decoding the original; different keys never wait on each
/// other.
pub struct ThumbnailService {
    pool: SqlitePool,
    data_dir: PathBuf,
    locks: KeyLocks,
    generations: AtomicU64,
}

impl ThumbnailService {
    pub fn new(pool: SqlitePool, data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            pool,
            data_dir,
            locks: StdMutex::new(HashMap::new()),
            generations: AtomicU64::new(0),
        })
    }

    /// The data directory originals are read from.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// How many thumbnails this service has generated (cache hits excluded).
    pub fn generations(&self) -> u64 {
        self.generations.load(Ordering::Relaxed)
    }

    /// The attachment and its thumbnail of exactly `size` (one of
    /// [`thumbnail::THUMB_SIZES`]), generating the thumbnail on a miss.
    /// `None` when the attachment does not exist, is not thumbnailable, or its
    /// original file is missing; `Err` only for unexpected failures (including
    /// an original that fails to decode).
    pub async fn get_or_generate(
        &self,
        attachment_id: &str,
        size: u32,
    ) -> Result<Option<(Attachment, Thumbnail)>> {
        let Some(att) = self.load_attachment(attachment_id)? else {
            return Ok(None);
        };
        if !thumbnail::is_thumbnailable(&att.mime_type) {
            return Ok(None);
        }
        let db_size = i32::try_from(size).context("thumbnail size out of range")?;
        if let Some(hit) = self.cached(attachment_id, db_size)? {
            return Ok(Some((att, hit)));
        }

        let lease = self.lease((attachment_id.to_owned(), size));
        let _guard = lease.lock.lock().await;
        // Another request may have generated it while this one waited.
        if let Some(hit) = self.cached(attachment_id, db_size)? {
            return Ok(Some((att, hit)));
        }
        let Some(generated) = self.generate(&att, size).await? else {
            return Ok(None);
        };
        let row = Thumbnail {
            attachment_id: att.id.clone(),
            size: db_size,
            mime_type: GENERATED_MIME.to_owned(),
            width: i32::try_from(generated.width).context("thumbnail width out of range")?,
            height: i32::try_from(generated.height).context("thumbnail height out of range")?,
            data: generated.data,
            created_at: Utc::now().naive_utc(),
        };
        let mut conn = self
            .pool
            .get()
            .context("db connection for thumbnail insert")?;
        attachment::insert_thumbnail(&mut conn, &row)?;
        Ok(Some((att, row)))
    }

    /// Decodes and resizes `att`'s original off the async runtime. `None`
    /// when the original file is missing.
    async fn generate(&self, att: &Attachment, size: u32) -> Result<Option<thumbnail::Generated>> {
        let path = attachment::original_path(&self.data_dir, &att.sha256);
        let bytes = match tokio::fs::read(&path).await {
            Err(err) if err.kind() == ErrorKind::NotFound => {
                tracing::warn!(id = %att.id, path = %path.display(), "original file missing");
                return Ok(None);
            }
            read => read.with_context(|| format!("reading {}", path.display()))?,
        };
        let started = Instant::now();
        let generated = task::spawn_blocking(move || thumbnail::generate_bytes(&bytes, size))
            .await
            .context("thumbnail task panicked")?
            .with_context(|| format!("generating the {size}px thumbnail of {:?}", att.id))?;
        self.generations.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            id = %att.id,
            size,
            elapsed_ms = started.elapsed().as_millis(),
            "generated thumbnail"
        );
        Ok(Some(generated))
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

    fn load_attachment(&self, id: &str) -> Result<Option<Attachment>> {
        let mut conn = self.pool.get().context("db connection")?;
        attachment::get(&mut conn, id)
    }

    fn cached(&self, id: &str, size: i32) -> Result<Option<Thumbnail>> {
        let mut conn = self.pool.get().context("db connection")?;
        attachment::thumbnail(&mut conn, id, size)
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

    use image::{ExtendedColorType, ImageEncoder, RgbImage};
    use tokio::time;

    use super::*;
    use crate::db::TestDb;
    use crate::kinds::AttachmentKind;
    use crate::schema::attachments;
    use crate::svc::fixtures::{attachment as attachment_row, seed_sample};
    use diesel::prelude::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// A service over the sample data plus attachment `att-1`, a 64×48 JPEG.
    fn service() -> (Arc<ThumbnailService>, TestDb, tempfile::TempDir) {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let data = tempfile::tempdir().unwrap();
        let path = attachment::original_path(data.path(), SHA);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .write_image(&RgbImage::new(64, 48), 64, 48, ExtendedColorType::Rgb8)
            .unwrap();
        fs::write(path, jpeg).unwrap();
        diesel::insert_into(attachments::table)
            .values(attachment_row(
                "att-1",
                &ids.drill,
                AttachmentKind::Photo,
                false,
                SHA,
                1,
                100,
            ))
            .execute(&mut conn)
            .unwrap();
        let svc = ThumbnailService::new(db.pool.clone(), data.path().to_path_buf());
        (svc, db, data)
    }

    #[tokio::test]
    async fn a_held_key_blocks_only_that_key() {
        // Review focus 1: different sizes must not serialize behind each other.
        let (svc, _db, _data) = service();
        let held = svc.lease(("att-1".to_owned(), 300));
        let _guard = held.lock.lock().await;

        let other = time::timeout(Duration::from_secs(10), svc.get_or_generate("att-1", 500))
            .await
            .expect("a different size must not wait for the held key")
            .unwrap();
        assert_eq!(other.unwrap().1.size, 500);

        let same = time::timeout(
            Duration::from_millis(200),
            svc.get_or_generate("att-1", 300),
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
        svc.get_or_generate("att-1", 300).await.unwrap().unwrap();
        // A missing original is the other early-return path.
        assert!(
            svc.get_or_generate("a-drill-photo", 300)
                .await
                .unwrap()
                .is_none()
        );
        assert!(svc.locks.lock().unwrap().is_empty());
    }
}
