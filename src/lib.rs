#![cfg_attr(docsrs, feature(doc_cfg))]

//! Asynchronous parser and writer for Electron's asar archive format.
//!
//! Requires Tokio runtime.
//!
//! Currently supported:
//! - Parse archive from file or async reader
//! - Pack archive from multiple readers, or conveniently from a folder.
//!
//! Currently not supported:
//! - Write and check integrity (planned)
//! - Unpacked files (planned)
//! - [`FileMetadata::executable`](header::FileMetadata::executable) (not
//!   planned, it is up to you whether use it or not)

pub mod header;

mod archive;
mod writer;

pub use archive::{Archive, Duplicable, File, LocalDuplicable};
pub use writer::Writer;

cfg_fs! {
  mod extract;

  pub use archive::DuplicableFile;
  pub use writer::pack_dir;
}

fn split_path(path: &str) -> Vec<&str> {
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

mod private {
  pub trait Sealed {}
  impl<T> Sealed for T {}
}

#[macro_export]
#[doc(hidden)]
macro_rules! cfg_fs {
  ($($item:item)*) => {
    $(
      #[cfg(feature = "fs")]
      #[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
      $item
    )*
  }
}
