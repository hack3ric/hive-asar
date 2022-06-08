use crate::header::{Directory, Entry, FileMetadata};
use std::future::Future;
use std::io::SeekFrom;
use std::path::Path;
use std::pin::Pin;
use tokio::fs::{create_dir, File as TokioFile};
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};

pub fn extract_entry<'a, R>(
  reader: &'a mut R,
  offset: u64,
  name: &'a str,
  entry: &'a Entry,
  path: &'a Path,
) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + Sync + 'a>>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
  Box::pin(async move {
    match entry {
      Entry::File(file) => extract_file(reader, offset, name, file, path).await?,
      Entry::Directory(dir) => extract_dir(reader, offset, name, dir, path).await?,
    }
    Ok(())
  })
}

async fn extract_file<R>(
  reader: &mut R,
  offset: u64,
  name: &str,
  file: &FileMetadata,
  path: &Path,
) -> io::Result<()>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
  reader.seek(SeekFrom::Start(offset + file.offset)).await?;
  let mut dest = TokioFile::create(path.join(name)).await?;
  io::copy(&mut reader.take(file.size), &mut dest).await?;
  Ok(())
}

async fn extract_dir<R>(
  reader: &mut R,
  offset: u64,
  name: &str,
  dir: &Directory,
  path: &Path,
) -> io::Result<()>
where
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin,
{
  let new_dir_path = path.join(name);
  create_dir(&new_dir_path).await?;
  for (name, entry) in dir.files.iter() {
    extract_entry(reader, offset, name, entry, &new_dir_path).await?;
  }
  Ok(())
}
