use crate::header::{Directory, Entry, FileMetadata};
use crate::private::Sealed;
use crate::{cfg_fs, split_path};
use async_trait::async_trait;
use std::io::{Cursor, SeekFrom};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, Take};

cfg_fs! {
  use pin_project::pin_project;
  use std::path::{Path, PathBuf};
  use tokio::fs::File as TokioFile;
}

/// Generic asar archive reader.
///
/// It supports any reader that implements [`AsyncRead`], [`AsyncSeek`] and
/// [`Unpin`], and adds more methods if the reader implements [`Send`] or
/// ([`Local`](LocalDuplicable))[`Duplicable`].
#[derive(Debug)]
pub struct Archive<R: AsyncRead + AsyncSeek + Unpin> {
  pub(crate) offset: u64,
  pub(crate) header: Directory,
  pub(crate) reader: R,
}

impl<R: AsyncRead + AsyncSeek + Unpin> Archive<R> {
  /// Parses an asar archive into `Archive`.
  pub async fn new(mut reader: R) -> io::Result<Self> {
    reader.seek(SeekFrom::Start(12)).await?;
    let header_size = reader.read_u32_le().await?;

    let mut header_bytes = vec![0; header_size as _];
    reader.read_exact(&mut header_bytes).await?;

    let header = serde_json::from_slice(&header_bytes).unwrap();
    let offset = match header_size % 4 {
      0 => header_size + 16,
      r => header_size + 16 + 4 - r,
    } as u64;

    Ok(Self {
      offset,
      header,
      reader,
    })
  }
}

cfg_fs! {
  impl Archive<TokioFileWithPath> {
    /// Opens a file and parses it into `Archive`.
    pub async fn new_from_file(path: impl Into<PathBuf>) -> io::Result<Self> {
      Self::new(TokioFileWithPath::open(path).await?).await
    }
  }
}

impl<R: AsyncRead + AsyncSeek + Unpin> Archive<R> {
  /// Reads a file entry from the archive by taking mutable reference.
  pub async fn read(&mut self, path: &str) -> io::Result<File<&mut R>> {
    let entry = self.header.search_segments(&split_path(path));
    match entry {
      Some(Entry::File(metadata)) => {
        (self.reader)
          .seek(SeekFrom::Start(self.offset + metadata.offset))
          .await?;
        Ok(File {
          offset: self.offset,
          metadata: metadata.clone(),
          content: (&mut self.reader).take(metadata.size),
        })
      }
      Some(_) => Err(io::Error::new(io::ErrorKind::Other, "not a file")),
      None => Err(io::ErrorKind::NotFound.into()),
    }
  }
}

macro_rules! impl_read_owned {
  (
    $(#[$attr:ident $($args:tt)*])*
    $read_owned:ident,
    $duplicate:ident $(,)?
  ) => {
    impl<R: AsyncRead + AsyncSeek + $duplicate + Unpin> Archive<R> {
      $(#[$attr $($args)*])*
      pub async fn $read_owned(&self, path: &str) -> io::Result<File<R>> {
        let entry = self.header.search_segments(&split_path(path));
        match entry {
          Some(Entry::File(metadata)) => {
            let mut file = self.reader.duplicate().await?;
            let seek_from = SeekFrom::Start(self.offset + metadata.offset);
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
  }
}

impl_read_owned! {
  /// Reads a file entry from the archive by duplicating the inner reader.
  ///
  /// Contrary to [`Archive::read`], it allows multiple read access over a single
  /// archive by creating a new file handle for every file. Useful when building a
  /// virtual file system like how Electron does.
  read_owned,
  Duplicable,
}

impl_read_owned! {
  /// Reads a file entry from the archive by duplicating the inner reader, without `Sync`.
  ///
  /// See [`Archive::read_owned`] for more information.
  read_owned_local,
  LocalDuplicable,
}

cfg_fs! {
  impl<R: AsyncRead + AsyncSeek + Send + Unpin> Archive<R> {
    /// Extracts the archive to a folder.
    pub async fn extract(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
      let path = path.as_ref();
      for (name, entry) in self.header.files.iter() {
        crate::extract::extract_entry(&mut self.reader, self.offset, name, entry, path).await?;
      }
      Ok(())
    }
  }

  impl<R: AsyncRead + AsyncSeek + Unpin> Archive<R> {
    /// Extracts the archive to a folder.
    ///
    /// This method is intended for `R: !Send`. Otherwise, use
    /// [`Archive::extract`] instead.
    pub async fn extract_local(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
      let path = path.as_ref();
      for (name, entry) in self.header.files.iter() {
        crate::extract::extract_entry_local(&mut self.reader, self.offset, name, entry, path).await?;
      }
      Ok(())
    }
  }
}

/// File from an asar archive.
pub struct File<R: AsyncRead + AsyncSeek + Unpin> {
  pub(crate) offset: u64,
  pub(crate) metadata: FileMetadata,
  pub(crate) content: Take<R>,
}

impl<R: AsyncRead + AsyncSeek + Unpin> File<R> {
  /// Gets the metadata of the file.
  pub fn metadata(&self) -> &FileMetadata {
    &self.metadata
  }
}

impl<R: AsyncRead + AsyncSeek + Unpin> AsyncRead for File<R> {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut io::ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    Pin::new(&mut self.content).poll_read(cx, buf)
  }
}

impl<R: AsyncRead + AsyncSeek + Unpin> AsyncSeek for File<R> {
  fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
    let current_relative_pos = self.metadata.size - self.content.limit();
    let offset = self.offset + self.metadata.offset;
    let absolute_pos = match position {
      SeekFrom::Start(pos) => SeekFrom::Start(offset + self.metadata.size.min(pos)),
      SeekFrom::Current(pos) if -pos as u64 > current_relative_pos => {
        return Err(io::ErrorKind::InvalidInput.into())
      }
      SeekFrom::Current(pos) => {
        let relative_pos = pos.min((self.metadata.size - current_relative_pos) as i64);
        SeekFrom::Current(relative_pos)
      }
      SeekFrom::End(pos) if pos > 0 => SeekFrom::Start(offset + self.metadata.size),
      SeekFrom::End(pos) if -pos as u64 > self.metadata.size => {
        return Err(io::ErrorKind::InvalidInput.into())
      }
      SeekFrom::End(pos) => SeekFrom::Start(offset + self.metadata.size - (-pos as u64)),
    };
    Pin::new(self.content.get_mut()).start_seek(absolute_pos)
  }

  fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
    let result = Pin::new(self.content.get_mut()).poll_complete(cx);
    match result {
      Poll::Ready(Ok(result)) => {
        let new_relative_pos = result - self.offset - self.metadata.offset;
        let new_limit = self.metadata.size - new_relative_pos;
        self.content.set_limit(new_limit);
        Poll::Ready(Ok(new_relative_pos))
      }
      other => other,
    }
  }
}

/// Ability to duplicate asynchronously.
///
/// [`Duplicable`] is like `Clone` with `async` and [`io::Result`]. However,
/// resulting object **must not share common state** with the original one.
///
/// This trait is currently for internal use only. You should not rely on
/// its implementations.
#[async_trait]
pub trait Duplicable: Sealed + Sized {
  async fn duplicate(&self) -> io::Result<Self>;
}

#[async_trait]
impl<T: Clone + Sync> Duplicable for Cursor<T> {
  async fn duplicate(&self) -> io::Result<Self> {
    Ok(self.clone())
  }
}

/// Ability to duplicate asynchronously without `Sync`.
///
/// See [`Duplicable`] for more information.
#[async_trait(?Send)]
pub trait LocalDuplicable: Sealed + Sized {
  async fn duplicate(&self) -> io::Result<Self>;
}

#[async_trait(?Send)]
impl<T: Clone> LocalDuplicable for Cursor<T> {
  async fn duplicate(&self) -> io::Result<Self> {
    Ok(self.clone())
  }
}

cfg_fs! {
  /// [`TokioFile`] with path.
  ///
  /// A new file handle with different internal state cannot be created from an
  /// existing one. [`TokioFile::try_clone`] shares its internal cursor,
  /// and thus cannot be [`Duplicable`]. `TokioFileWithPath`, however, opens a
  /// new file handle every time [`Duplicable::duplicate`] is called.
  #[pin_project]
  pub struct TokioFileWithPath {
    #[pin]
    inner: TokioFile,
    path: PathBuf,
  }

  impl TokioFileWithPath {
    pub async fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
      let path = path.into();
      let inner = TokioFile::open(&path).await?;
      Ok(Self { inner, path })
    }

    pub async fn path(&self) -> &Path {
      &self.path
    }

    pub async fn into_inner(self) -> (TokioFile, PathBuf) {
      (self.inner, self.path)
    }
  }

  impl AsyncRead for TokioFileWithPath {
    fn poll_read(
      self: Pin<&mut Self>,
      cx: &mut Context<'_>,
      buf: &mut io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
      self.project().inner.poll_read(cx, buf)
    }
  }

  impl AsyncSeek for TokioFileWithPath {
    fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
      self.project().inner.start_seek(position)
    }

    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<u64>> {
      self.project().inner.poll_complete(cx)
    }
  }

  #[async_trait]
  impl Duplicable for TokioFileWithPath {
    async fn duplicate(&self) -> io::Result<Self> {
      Ok(Self {
        inner: TokioFile::open(&self.path).await?,
        path: self.path.clone(),
      })
    }
  }

  #[async_trait(?Send)]
  impl LocalDuplicable for TokioFileWithPath {
    async fn duplicate(&self) -> io::Result<Self> {
      Ok(Self {
        inner: TokioFile::open(&self.path).await?,
        path: self.path.clone(),
      })
    }
  }
}
