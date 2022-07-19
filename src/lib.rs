#![cfg_attr(docsrs, feature(doc_cfg))]

//! Asynchronous parser and writer for Electron's asar archive format.
//!
//! Requires Tokio runtime.
//!
//! Currently supported:
//! - Parse archive from file or async reader
//! - Pack archive from multiple readers, or conveniently from a folder.
//! - Write and check integrity
//!
//! Currently not supported:
//! - Unpacked files

pub mod header;

mod archive;
mod writer;

pub use archive::{check_asar_format, Archive, Duplicable, File, LocalDuplicable};
pub use writer::Writer;

cfg_fs! {
  mod extract;

  pub use archive::DuplicableFile;
  pub use writer::{pack_dir, pack_dir_into_writer};

  cfg_stream! {
    pub use writer::pack_dir_into_stream;
  }
}

cfg_integrity! {
  const BLOCK_SIZE: u32 = 4_194_304;
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

#[macro_export]
#[doc(hidden)]
macro_rules! cfg_integrity {
  ($($item:item)*) => {
    $(
      #[cfg(feature = "integrity")]
      #[cfg_attr(docsrs, doc(cfg(feature = "integrity")))]
      $item
    )*
  }
}

#[macro_export]
#[doc(hidden)]
macro_rules! cfg_stream {
  ($($item:item)*) => {
    $(
      #[cfg(feature = "stream")]
      #[cfg_attr(docsrs, doc(cfg(feature = "stream")))]
      $item
    )*
  }
}
