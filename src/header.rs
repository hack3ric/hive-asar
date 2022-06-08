//! Structures that describes asar's header.
//!
//! asar's header is represented using a single root [`Directory`], with tree
//! structures similar to what the file system looks like.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Entry of either a file or a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Entry {
  /// A file.
  File(FileMetadata),

  /// A directory.
  Directory(Directory),
}

impl Entry {
  pub(crate) fn search_segments(&self, segments: &[&str]) -> Option<&Entry> {
    match self {
      _ if segments.is_empty() => Some(self),
      Self::File(_) => None,
      Self::Directory(dir) => dir.search_segments(segments),
    }
  }
}

/// Metadata of a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
  /// The file's offset position after header.
  #[serde(with = "serde_offset")]
  pub offset: u64,

  /// The file's size.
  ///
  /// According to [official repository], this field should not be larger than
  /// `9007199254740991`, which is JavaScript's `Number.MAX_SAFE_INTEGER` and about 8PB in size.
  /// However, if you do not need to interact with the official implementation,
  /// any `u64` value would be OK.
  ///
  /// [official repository]: https://github.com/electron/asar#format
  pub size: u64,

  /// Whether the file is an executable.
  #[serde(default)]
  pub executable: bool,

  /// Optional integrity information of the file.
  pub integrity: Option<Integrity>,
}

/// A directory, containing files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Directory {
  pub files: HashMap<Box<str>, Entry>,
}

impl Directory {
  pub(crate) fn search_segments(&self, segments: &[&str]) -> Option<&Entry> {
    (self.files)
      .get(segments[0])
      .and_then(|x| x.search_segments(&segments[1..]))
  }
}

/// Integrity information of a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integrity {
  /// Hashing algorithm used.
  pub algorithm: Algorithm,

  /// The hash of the entire file.
  pub hash: String,

  /// Indicates the size of each block of the hashes in `blocks`.
  #[serde(rename = "blockSize")]
  pub block_size: u32,

  /// Hashes of blocks.
  pub blocks: Vec<String>,
}

/// Hashing algorithm used to check files' integrity.
///
/// Currently only SHA256 is officially supported.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Algorithm {
  SHA256,
}

mod serde_offset {
  use serde::de::Error;
  use serde::{Deserialize, Deserializer, Serializer};

  pub fn serialize<S: Serializer>(offset: &u64, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(&offset.to_string())
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<u64, D::Error> {
    String::deserialize(de)?.parse().map_err(D::Error::custom)
  }
}
