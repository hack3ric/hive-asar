use crate::header::{Directory, Entry, FileMetadata, FilePosition, Integrity};
use crate::{cfg_fs, cfg_integrity, cfg_stream, split_path};
use std::io::SeekFrom;
use tokio::io::{
  self, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt, Take,
};

cfg_fs! {
  use std::future::Future;
  use std::path::Path;
  use std::pin::Pin;
  use tokio::fs::{read_dir, symlink_metadata, File as TokioFile};
}

cfg_integrity! {
  use crate::BLOCK_SIZE;
  use crate::header::{Algorithm, Hash};
  use sha2::digest::Digest;
  use sha2::Sha256;
}

cfg_stream! {
  use bytes::Bytes;
  use futures_core::Stream;
  use futures_util::future::ok;
  use futures_util::stream::{iter, once};
  use futures_util::StreamExt;
  use tokio_util::io::ReaderStream;
}

/// Asar archive writer.
#[derive(Debug)]
pub struct Writer<F: AsyncRead + Unpin> {
  header: Directory,
  file_offset: u64,
  files: Vec<Take<F>>,
}

impl<F: AsyncRead + Unpin> Writer<F> {
  /// Creates a new, empty archive writer.
  pub fn new() -> Self {
    Default::default()
  }

  fn add_folder_recursively(&mut self, segments: Vec<&str>) -> &mut Directory {
    let mut dir = &mut self.header;
    for seg in segments {
      let entry = (dir.files)
        .entry(seg.into())
        .or_insert_with(|| Entry::Directory(Default::default()));
      dir = match entry {
        Entry::Directory(dir) => dir,
        _ => unreachable!(),
      }
    }
    dir
  }

  /// Add an entry to the archive.
  ///
  /// The entry's parent directories are created recursively if they do not
  /// exist.
  ///
  /// `size` should correspond with `content`. If `size` is smaller, exactly
  /// `size` bytes will be written. If `size` is bigger, the
  /// [`Writer::write`] method will fail. For convenience, you may want
  /// to use `add_sized`.
  ///
  /// # Panic
  ///
  /// The method panics if normalised `path` contains no filename, or if the
  /// path is already occupied by a previously inserted file.
  pub fn add(&mut self, path: &str, content: F, size: u64) {
    self.add_with_options(path, content, size, false, None)
  }

  fn add_with_options(
    &mut self,
    path: &str,
    content: F,
    size: u64,
    executable: bool,
    integrity: Option<Integrity>,
  ) {
    let mut segments = split_path(path);
    let filename = segments
      .pop()
      .expect("normalised path contains no filename");
    let file_entry = FileMetadata {
      pos: FilePosition::Offset(self.file_offset),
      size,
      executable,
      integrity,
    };
    let result = self
      .add_folder_recursively(segments)
      .files
      .insert(filename.into(), Entry::File(file_entry));
    assert!(result.is_none());
    self.file_offset += size;
    self.files.push(content.take(size))
  }

  /// Adds an empty folder recursively to the archive.
  pub fn add_empty_folder(&mut self, path: &str) {
    self.add_folder_recursively(split_path(path));
  }

  /// Finishes the archive and writes the content into `dest`.
  pub async fn write(self, dest: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
    let header_bytes = serde_json::to_vec(&self.header)?;
    let header_len = header_bytes.len() as u32;
    let padding = match header_len % 4 {
      0 => 0,
      r => 4 - r,
    };

    dest.write_u32_le(4).await?;
    dest.write_u32_le(header_len + padding + 8).await?;
    dest.write_u32_le(header_len + padding + 4).await?;
    dest.write_u32_le(header_len).await?;

    dest.write_all(&header_bytes).await?;
    dest.write_all(&vec![0; padding as _]).await?;

    for mut file in self.files {
      io::copy(&mut file, dest).await?;
    }

    Ok(())
  }

  cfg_stream! {
    pub fn into_stream(self) -> io::Result<impl Stream<Item = io::Result<Bytes>>> {
      let mut header_bytes = serde_json::to_vec(&self.header)?;
      let header_len = header_bytes.len() as u32;
      let padding = match header_len % 4 {
        0 => 0,
        r => 4 - r,
      };
      for _ in 0..padding {
        header_bytes.extend(Some(0))
      }

      let mut header_meta = Vec::with_capacity((16 + header_len + padding) as _);
      for i in [
        4u32,
        header_len + padding + 8,
        header_len + padding + 4,
        header_len,
      ] {
        header_meta.extend(i.to_le_bytes());
      }

      let stream = once(ok(header_meta.into()))
        .chain(once(ok(header_bytes.into())))
        .chain(iter(self.files.into_iter().map(ReaderStream::new)).flatten());
      Ok(stream)
    }
  }
}

impl<F: AsyncRead + AsyncSeek + Unpin> Writer<F> {
  /// Add an entry to the archive.
  ///
  /// Similar to [`Writer::add`], but it uses [`AsyncSeekExt::seek`] to
  /// determine the size of the content.
  ///
  /// For more information see [`Writer::add`].
  pub async fn add_sized(&mut self, path: &str, mut content: F) -> io::Result<()> {
    let size = content.seek(SeekFrom::End(0)).await? - content.stream_position().await?;
    self.add(path, content, size);
    Ok(())
  }

  cfg_integrity! {
    pub async fn add_sized_with_integrity(&mut self, path: &str, mut content: F) -> io::Result<()> {
      let mut global_state = Sha256::new();
      let mut block = Vec::with_capacity(BLOCK_SIZE as _);
      let mut blocks = Vec::new();
      let mut size = 0;
      loop {
        let read_size = (&mut content)
          .take(BLOCK_SIZE as _)
          .read_to_end(&mut block)
          .await?;
        if read_size == 0 {
          break;
        }
        size += read_size;
        blocks.push(Hash(Sha256::digest(&block).to_vec()));
        global_state.update(&block);
        block.clear();
      }
      let integrity = Integrity {
        algorithm: Algorithm::SHA256,
        hash: Hash(global_state.finalize().to_vec()),
        block_size: BLOCK_SIZE,
        blocks,
      };
      content.rewind().await?;
      self.add_with_options(path, content, size as _, false, Some(integrity));
      Ok(())
    }
  }
}

impl<F: AsyncRead + Unpin> Default for Writer<F> {
  fn default() -> Self {
    Self {
      header: Default::default(),
      file_offset: 0,
      files: Vec::new(),
    }
  }
}

cfg_fs! {
  /// Pack a directory to asar archive.
  pub async fn pack_dir(
    path: impl AsRef<Path>,
    dest: &mut (impl AsyncWrite + Unpin),
  ) -> io::Result<()> {
    pack_dir_into_writer(path)
      .await?
      .write(dest)
      .await
  }

  cfg_stream! {
    pub async fn pack_dir_into_stream(
      path: impl AsRef<Path>,
    ) -> io::Result<impl Stream<Item = io::Result<Bytes>>> {
      pack_dir_into_writer(path)
        .await?
        .into_stream()
    }
  }

  pub async fn pack_dir_into_writer(
    path: impl AsRef<Path>,
  ) -> io::Result<Writer<TokioFile>> {
    let path = path.as_ref().canonicalize()?;
    let mut writer = Writer::<TokioFile>::new();
    add_dir_files(&mut writer, &path, &path).await?;
    Ok(writer)
  }

  fn add_dir_files<'a>(
    writer: &'a mut Writer<TokioFile>,
    path: &'a Path,
    original_path: &'a Path,
  ) -> Pin<Box<dyn Future<Output = io::Result<()>> + 'a>> {
    Box::pin(async move {
      if symlink_metadata(path).await?.is_dir() {
        let mut rd = read_dir(path).await?;
        while let Some(entry) = rd.next_entry().await? {
          let file_type = entry.file_type().await?;
          if file_type.is_dir() {
            add_dir_files(writer, &entry.path(), original_path).await?;
          } else if file_type.is_symlink() {
            // do nothing
          } else {
            let absolute_path = entry.path();
            let file = TokioFile::open(&absolute_path).await?;
            let relative_path = absolute_path
              .strip_prefix(original_path)
              .unwrap()
              .to_str()
              .unwrap();
            #[cfg(not(feature = "integrity"))]
            writer.add_sized(relative_path, file).await?;
            #[cfg(feature = "integrity")]
            writer.add_sized_with_integrity(relative_path, file).await?;
          }
        }
      }
      Ok(())
    })
  }
}
