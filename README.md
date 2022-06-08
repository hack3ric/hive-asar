# hive-asar

Asynchronous parser and writer for Electron's asar archive format.

Requires Tokio runtime.

Currently supported:
- Parse archive from file or any reader that implements `AsyncRead + AsyncSeek + Send + Sync + Unpin`
- Pack archive from multiple readers, or conveniently from a folder.

Currently not supported:
- Write and check integrity (planned)
- [`FileMetadata::executable`](header::FileMetadata::executable) (not planned, it is up to you whether use it or not)

## License

`hive-asar` is licensed under the MIT License.
