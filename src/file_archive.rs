use crate::header::Entry;
use crate::{split_path, Archive, File};
use std::io::SeekFrom;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use tokio::fs::File as TokioFile;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// File-based asar archive reader, allowing multiple read access at a time
/// through `read_owned`.
///
/// It implements `Deref<Target = Archive>`, so `Archive`'s methods can still be
/// used.
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
pub struct FileArchive {
  archive: Archive<TokioFile>,
  path: PathBuf,
}

impl FileArchive {
  /// Parses an asar archive into `FileArchive`.
  pub async fn new(path: impl Into<PathBuf>) -> io::Result<Self> {
    let path = path.into();
    let file = TokioFile::open(&path).await?;
    Ok(Self {
      archive: Archive::new(file).await?,
      path,
    })
  }

  /// Reads a file entry from the archive.
  ///
  /// Contrary to `Archive::read`, it allows multiple read access over a single
  /// archive by creating a new file handle for every file.
  pub async fn read_owned(&self, path: &str) -> io::Result<File<TokioFile>> {
    let entry = self.archive.header.search_segments(&split_path(path));
    match entry {
      Some(Entry::File(metadata)) => {
        let mut file = TokioFile::open(&self.path).await?;
        let seek_from = SeekFrom::Start(self.archive.offset + metadata.offset);
        file.seek(seek_from).await?;
        Ok(File {
          offset: self.offset,
          metadata: metadata.clone(),
          content: file.take(metadata.size),
        })
      }
      Some(_) => Err(io::Error::new(io::ErrorKind::Other, "not a file")),
      None => Err(io::ErrorKind::NotFound.into()),
    }
  }
}

impl Deref for FileArchive {
  type Target = Archive<TokioFile>;

  fn deref(&self) -> &Self::Target {
    &self.archive
  }
}

impl DerefMut for FileArchive {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.archive
  }
}
