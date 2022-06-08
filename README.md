# hive-asar

[![Rust](https://github.com/hack3ric/hive-asar/actions/workflows/rust.yml/badge.svg)](https://github.com/hack3ric/hive-asar/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/hive-asar)](https://crates.io/crates/hive-asar)
[![Docs](https://img.shields.io/docsrs/hive-asar)](https://docs.rs/hive-asar)
[![License](https://img.shields.io/crates/l/hive-asar)](LICENSE)

Asynchronous parser and writer for Electron's asar archive format.

Requires Tokio 1.x runtime.

Currently supported:
- Parse archive from file or async reader
- Pack archive from multiple readers, or conveniently from a folder

Currently not supported:
- Write and check integrity (planned)
- Unpacked files (planned)
- `executable` (not planned, it is up to you whether use it or not)

## Examples

### Reading

```rust
use hive_asar::{Archive, FileArchive};
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
  // Parses an asar archive from a file
  let mut file_archive = Archive::new_from_file("path/to/archive.asar").await?;

  // Gets the file, retrieving its metadata and reading the entire content
  let mut file = file_archive.read("path/to/file.txt").await?;
  let size = file.metadata().size;
  let mut buf = Vec::with_capacity(size as _);
  file.read_to_end(&mut buf).await?;

  Ok(())
}
```

### Writing

```rust
use hive_asar::{Writer, pack_dir};
use tokio::io::AsyncReadExt;
use tokio::fs::File;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
  let mut writer = Writer::new();

  // You can manually add all of the file one by one
  writer.add_sized("foo.txt", File::open("folder/foo.txt").await?).await?;
  writer.add_sized("bar.toml", File::open("folder/bar.toml").await?).await?;
  writer.add_sized("baaz.rs", File::open("folder/baaz.rs").await?).await?;
  writer.write(File::create("dest.asar").await?).await?;

  // Or use `pack_dir` to pack a directory's content conveniently
  pack_dir("folder", File::create("dest.asar").await?).await?;

  Ok(())
}
```

## Features

- `fs`: (enabled by default) File system support, e.g. `FileArchive`, `Archive::extract` and `pack_dir`

## License

`hive-asar` is licensed under the MIT License.
