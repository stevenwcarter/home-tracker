//! Where a backup's bytes come from: an exploded directory or the zip itself.

use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use zip::ZipArchive;
use zip::result::ZipError;

/// A Homebox backup's tables and attachment blobs, however they are stored.
pub trait Source {
    /// `read_table("entities")` returns the bytes of `entities.json`, or `None` if absent.
    fn read_table(&mut self, name: &str) -> Result<Option<Vec<u8>>>;
    /// The raw blob stored under `attachments/<id>`, or `None` if absent.
    fn read_attachment(&mut self, id: &str) -> Result<Option<Vec<u8>>>;
    /// A human-readable description of where the backup lives, for logs.
    fn describe(&self) -> String;
}

/// A backup that has already been unzipped into a directory.
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn read_optional(&self, rel: &Path) -> Result<Option<Vec<u8>>> {
        let path = self.root.join(rel);
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }
}

impl Source for DirSource {
    fn read_table(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        self.read_optional(Path::new(&format!("{name}.json")))
    }

    fn read_attachment(&mut self, id: &str) -> Result<Option<Vec<u8>>> {
        self.read_optional(&Path::new("attachments").join(id))
    }

    fn describe(&self) -> String {
        format!("directory {}", self.root.display())
    }
}

/// A backup read straight out of the `.zip` Homebox produces.
pub struct ZipSource {
    archive: ZipArchive<File>,
    path: PathBuf,
}

impl ZipSource {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
        let archive = ZipArchive::new(file)
            .with_context(|| format!("{} is not a zip file", path.display()))?;
        Ok(Self { archive, path })
    }

    fn read_entry(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        let mut entry = match self.archive.by_name(name) {
            Ok(entry) => entry,
            Err(ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("reading {name} from the zip")),
        };
        // The size is only a capacity hint; a bogus header value just reallocates.
        let mut buf = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or_default());
        entry
            .read_to_end(&mut buf)
            .with_context(|| format!("decompressing {name} from the zip"))?;
        Ok(Some(buf))
    }
}

impl Source for ZipSource {
    fn read_table(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        self.read_entry(&format!("{name}.json"))
    }

    fn read_attachment(&mut self, id: &str) -> Result<Option<Vec<u8>>> {
        self.read_entry(&format!("attachments/{id}"))
    }

    fn describe(&self) -> String {
        format!("zip {}", self.path.display())
    }
}

/// A `.zip` file becomes a `ZipSource`; a directory becomes a `DirSource`.
pub fn open(path: &Path) -> Result<Box<dyn Source>> {
    if path.is_dir() {
        Ok(Box::new(DirSource::new(path)))
    } else if path.is_file() {
        Ok(Box::new(ZipSource::open(path)?))
    } else {
        bail!("{} is neither a directory nor a file", path.display())
    }
}
