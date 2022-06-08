use crate::header::{Directory, Entry, FileMetadata};
use std::future::Future;
use std::io::SeekFrom;
use std::path::Path;
use std::pin::Pin;
use tokio::fs::{create_dir, File as TokioFile};
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};

macro_rules! impl_extract_entry {
  (
    $extract_entry:ident,
    $extract_dir:ident
    $(, $send:ident)?
  ) => {
    pub fn $extract_entry<'a, R: AsyncRead + AsyncSeek $(+ $send)? + Unpin>(
      reader: &'a mut R,
      offset: u64,
      name: &'a str,
      entry: &'a Entry,
      path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> $(+ $send)? + 'a>> {
      Box::pin(async move {
        match entry {
          Entry::File(file) => extract_file(reader, offset, name, file, path).await?,
          Entry::Directory(dir) => $extract_dir(reader, offset, name, dir, path).await?,
        }
        Ok(())
      })
    }
  }
}

impl_extract_entry!(extract_entry, extract_dir, Send);
impl_extract_entry!(extract_entry_local, extract_dir_local);

async fn extract_file<R: AsyncRead + AsyncSeek + Unpin>(
  reader: &mut R,
  offset: u64,
  name: &str,
  file: &FileMetadata,
  path: &Path,
) -> io::Result<()> {
  reader.seek(SeekFrom::Start(offset + file.offset()?)).await?;
  let mut dest = TokioFile::create(path.join(name)).await?;
  io::copy(&mut reader.take(file.size), &mut dest).await?;
  Ok(())
}

macro_rules! impl_extract_dir {
  (
    $extract_dir:ident,
    $extract_entry:ident
    $(, $send:ident)?
  ) => {
    async fn $extract_dir<R: AsyncRead + AsyncSeek $(+ $send)? + Unpin>(
      reader: &mut R,
      offset: u64,
      name: &str,
      dir: &Directory,
      path: &Path,
    ) -> io::Result<()> {
      let new_dir_path = path.join(name);
      create_dir(&new_dir_path).await?;
      for (name, entry) in dir.files.iter() {
        $extract_entry(reader, offset, name, entry, &new_dir_path).await?;
      }
      Ok(())
    }
  }
}

impl_extract_dir!(extract_dir, extract_entry, Send);
impl_extract_dir!(extract_dir_local, extract_entry_local);
