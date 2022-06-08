#![cfg_attr(docsrs, feature(doc_cfg))]

//! Asynchronous parser and writer for Electron's asar archive format.
//!
//! Requires Tokio runtime.
//!
//! Currently supported:
//! - Parse archive from file or any reader that implements `AsyncRead +
//!   AsyncSeek + Send + Sync + Unpin`
//! - Pack archive from multiple readers, or conveniently from a folder.
//!
//! Currently not supported:
//! - Write and check integrity (planned)
//! - [`FileMetadata::executable`](header::FileMetadata::executable) (not
//!   planned, it is up to you whether use it or not)

pub mod header;

mod archive;
#[cfg(feature = "fs")]
mod extract;
#[cfg(feature = "fs")]
mod file_archive;
mod writer;

pub use archive::{Archive, File};
#[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
pub use file_archive::FileArchive;
#[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
pub use writer::pack_dir;
pub use writer::Writer;

pub(crate) fn split_path(path: &str) -> Vec<&str> {
  path
    .split('/')
    .filter(|x| !x.is_empty() && *x != ".")
    .fold(Vec::new(), |mut result, segment| {
      if segment == ".." {
        result.pop();
      } else {
        result.push(segment);
      }
      result
    })
}
