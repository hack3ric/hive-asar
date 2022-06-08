use crate::header::{Directory, Entry, FileMetadata};
use crate::split_path;
use std::io::SeekFrom;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, Take};

/// Asar archive reader.
#[derive(Debug)]
pub struct Archive<R>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
  pub(crate) offset: u64,
  pub(crate) header: Directory,
  pub(crate) reader: R,
}

impl<R> Archive<R>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
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

  /// Reads a file entry from the archive.
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

  /// Extracts the archive to a folder.
  #[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
  pub async fn extract(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    for (name, entry) in self.header.files.iter() {
      crate::extract::extract_entry(&mut self.reader, self.offset, name, entry, path).await?;
    }
    Ok(())
  }
}

/// File from an asar archive.
pub struct File<R>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
  pub(crate) offset: u64,
  pub(crate) metadata: FileMetadata,
  pub(crate) content: Take<R>,
}

impl<R> File<R>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
  /// Gets the metadata of the file.
  pub fn metadata(&self) -> &FileMetadata {
    &self.metadata
  }
}

impl<R> AsyncRead for File<R>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut io::ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    Pin::new(&mut self.content).poll_read(cx, buf)
  }
}

impl<R> AsyncSeek for File<R>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
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
