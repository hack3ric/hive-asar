//! Structures that describes asar's header.
//!
//! Asar's header is represented using a single root [`Directory`], with tree
//! structures similar to what the file system looks like.

use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Deref;
use tokio::io;

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
  /// Where the file is located.
  #[serde(flatten)]
  pub pos: FilePosition,

  /// The file's size.
  ///
  /// According to [official repository], this field should not be larger than
  /// `9007199254740991`, which is JavaScript's `Number.MAX_SAFE_INTEGER` and
  /// about 8PB in size.
  ///
  /// [official repository]: https://github.com/electron/asar#format
  pub size: u64,

  /// Whether the file is an executable.
  #[serde(default)]
  #[serde(skip_serializing_if = "bool::clone")]
  pub executable: bool,

  /// Optional integrity information of the file.
  pub integrity: Option<Integrity>,
}

impl FileMetadata {
  pub(crate) fn offset(&self) -> io::Result<u64> {
    if let FilePosition::Offset(x) = self.pos {
      Ok(x)
    } else {
      Err(io::Error::new(
        io::ErrorKind::Other,
        "unpacked file is currently not supported",
      ))
    }
  }
}

/// Whether the file is stored in the archive or is unpacked.
#[derive(Debug, Clone, Copy)]
pub enum FilePosition {
  /// Offset of the file in the archive, indicates the file is stored in it.
  Offset(u64),

  /// Indicates the file is stored outside the archive.
  Unpacked,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Helper<'a> {
  Offset {
    #[serde(skip_serializing_if = "Option::is_none")]
    unpacked: Option<bool>,
    offset: &'a str,
  },
  Unpacked {
    unpacked: bool,
  },
}

impl Serialize for FilePosition {
  fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
    let offset_string;
    let helper = match self {
      Self::Offset(offset) => {
        offset_string = offset.to_string();
        Helper::Offset {
          unpacked: None,
          offset: &offset_string,
        }
      }
      Self::Unpacked => Helper::Unpacked { unpacked: true },
    };

    helper.serialize(ser)
  }
}

impl<'de> Deserialize<'de> for FilePosition {
  fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
    match Helper::deserialize(de)? {
      Helper::Offset { unpacked, .. } if matches!(unpacked, Some(true)) => {
        Err(Error::custom("got both 'unpacked' and 'offset' field"))
      }
      Helper::Offset { offset, .. } => offset
        .parse()
        .map(Self::Offset)
        .map_err(|_| Error::invalid_value(Unexpected::Str(offset), &"valid u64 string")),
      Helper::Unpacked { unpacked: true } => Ok(Self::Unpacked),
      Helper::Unpacked { unpacked: false } => {
        Err(Error::invalid_value(Unexpected::Bool(false), &"true"))
      }
    }
  }
}

/// Integrity information of a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integrity {
  /// Hashing algorithm used.
  pub algorithm: Algorithm,

  /// The hash of the entire file.
  pub hash: Hash,

  /// Indicates the size of each block of the hashes in `blocks`.
  #[serde(rename = "blockSize")]
  pub block_size: u32,

  /// Hashes of blocks.
  pub blocks: Vec<Hash>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Hash(#[serde(with = "hex::serde")] pub(crate) Vec<u8>);

impl From<Vec<u8>> for Hash {
  fn from(x: Vec<u8>) -> Self {
    Self(x)
  }
}

impl From<Hash> for Vec<u8> {
  fn from(x: Hash) -> Self {
    x.0
  }
}

impl AsRef<[u8]> for Hash {
  fn as_ref(&self) -> &[u8] {
    &self.0
  }
}

impl Deref for Hash {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    self.as_ref()
  }
}

impl Debug for Hash {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    <Self as Display>::fmt(self, f)
  }
}

impl Display for Hash {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    f.write_str(&hex::encode(&self.0))
  }
}

/// Hashing algorithm used to check files' integrity.
///
/// Currently only SHA256 is officially supported.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Algorithm {
  SHA256,
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
