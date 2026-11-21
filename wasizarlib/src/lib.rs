/// Execute a pyzarlib-compatible archive operation with JSON input and output.
///
/// File paths are resolved by the WASI runtime and must be available through
/// its preopened directories.
pub fn execute_json(operation: &str, request_json: &str) -> Result<String, String> {
    czarlib::execute_json(operation, request_json)
}

#[cfg(all(target_os = "wasi", feature = "streaming"))]
mod component {
    use std::io::{self, Read, Seek, SeekFrom, Write};
    use zarlib::operations::{
        EncodingOptions, PasswordSet, StreamEncryption, StreamInput, create_archive_from_readers,
        extract_single_file_to_writer, parse_commit_id,
    };
    use zarlib::{ContentAead, EncryptionScope};

    wit_bindgen::generate!({
        path: "wit",
        world: "wasizarlib",
    });

    struct Wasizarlib;

    const IO_CHUNK: usize = 64 * 1024;

    struct HostStream(u32);

    impl Read for HostStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let max = buf.len().min(IO_CHUNK);
            let data =
                zar::wasizarlib::streams::read(self.0, max as u32).map_err(io::Error::other)?;
            if data.len() > max {
                return Err(io::Error::other("host read exceeded requested size"));
            }
            buf[..data.len()].copy_from_slice(&data);
            Ok(data.len())
        }
    }

    impl Write for HostStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let bytes = &buf[..buf.len().min(IO_CHUNK)];
            let count =
                zar::wasizarlib::streams::write(self.0, bytes).map_err(io::Error::other)? as usize;
            if count > bytes.len() {
                return Err(io::Error::other("host write exceeded supplied size"));
            }
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Seek for HostStream {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            use zar::wasizarlib::streams::SeekPosition;
            let position = match position {
                SeekFrom::Start(n) => SeekPosition::Start(n),
                SeekFrom::Current(n) => SeekPosition::Current(n),
                SeekFrom::End(n) => SeekPosition::End(n),
            };
            zar::wasizarlib::streams::seek(self.0, position).map_err(io::Error::other)
        }
    }

    impl exports::zar::wasizarlib::operations::Guest for Wasizarlib {
        fn execute(operation: String, request_json: String) -> Result<String, String> {
            super::execute_json(&operation, &request_json)
        }

        fn create_from_streams(
            archive: u32,
            inputs: Vec<exports::zar::wasizarlib::operations::StreamInput>,
            options: exports::zar::wasizarlib::operations::CreateOptions,
        ) -> Result<(), String> {
            use exports::zar::wasizarlib::operations::{Aead, EncryptionScope as Scope};
            let encryption = options.encryption.as_ref().map(|value| StreamEncryption {
                password: value.password.as_bytes(),
                algorithm: match value.algorithm {
                    Aead::Aes256Gcm => ContentAead::Aes256Gcm,
                    Aead::Chacha20Poly1305 => ContentAead::ChaCha20Poly1305,
                },
                scope: match value.scope {
                    Scope::Data => EncryptionScope::Data,
                    Scope::Header => EncryptionScope::Header,
                    Scope::HeaderAndData => EncryptionScope::HeaderAndData,
                },
            });
            let mut readers: Vec<_> = inputs
                .iter()
                .map(|input| HostStream(input.handle))
                .collect();
            let mut streams: Vec<_> = inputs
                .iter()
                .zip(&mut readers)
                .map(|(input, reader)| StreamInput {
                    path: &input.path,
                    reader,
                })
                .collect();
            create_archive_from_readers(
                &mut HostStream(archive),
                &mut streams,
                EncodingOptions {
                    block_size: options.block_size as usize,
                    compression_level: options.compression_level,
                },
                encryption,
            )
            .map_err(|error| error.to_string())
        }

        fn extract_to_stream(
            archive: u32,
            path: String,
            output: u32,
            password: Option<String>,
            passwords: Vec<exports::zar::wasizarlib::operations::CommitPassword>,
            to: Option<String>,
        ) -> Result<u64, String> {
            let mut set = PasswordSet::new(password.map(String::into_bytes));
            for entry in passwords {
                let id = parse_commit_id(&entry.commit).map_err(|error| error.to_string())?;
                set.insert_for_commit(id, entry.password.into_bytes());
            }
            extract_single_file_to_writer(
                &mut HostStream(archive),
                &path,
                &mut HostStream(output),
                &set,
                to.as_deref(),
            )
            .map_err(|error| error.to_string())
        }
    }

    export!(Wasizarlib);
}

#[cfg(all(target_os = "wasi", not(feature = "streaming")))]
mod component {
    wit_bindgen::generate!({
        path: "wit/legacy",
        world: "wasizarlib",
    });

    struct Wasizarlib;

    impl exports::zar::wasizarlib::operations::Guest for Wasizarlib {
        fn execute(operation: String, request_json: String) -> Result<String, String> {
            super::execute_json(&operation, &request_json)
        }
    }

    export!(Wasizarlib);
}

#[cfg(test)]
mod tests {
    use super::execute_json;

    #[test]
    fn execute_json_reports_invalid_requests() {
        assert!(
            execute_json("check_archive", "not-json")
                .unwrap_err()
                .starts_with("invalid request JSON:")
        );
    }
}
