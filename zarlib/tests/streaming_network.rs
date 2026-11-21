#![allow(clippy::duplicate_mod)]

use std::io::{self, Cursor, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::thread;

use zarlib::operations::{EncodingOptions, StreamInput, create_archive_from_readers};

#[allow(dead_code)]
#[path = "../examples/streaming_client.rs"]
mod client;
#[allow(dead_code)]
#[path = "../examples/streaming_server.rs"]
mod server;

fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    let mut readers: Vec<_> = entries.iter().map(|(_, data)| Cursor::new(*data)).collect();
    let mut inputs: Vec<_> = entries
        .iter()
        .zip(readers.iter_mut())
        .map(|((path, _), reader)| StreamInput { path, reader })
        .collect();
    let mut file = std::fs::File::create(path).unwrap();
    create_archive_from_readers(&mut file, &mut inputs, EncodingOptions::default(), None).unwrap();
}

struct MeasuredSocket {
    socket: TcpStream,
    received: usize,
}

impl Read for MeasuredSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.socket.read(buf)?;
        self.received += count;
        Ok(count)
    }
}

impl Write for MeasuredSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.socket.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}

fn request(
    root: &Path,
    archive: &str,
    output: &Path,
    patterns: &[&str],
) -> (Result<(), String>, usize) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let root = root.canonicalize().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        server::serve_directory_request(&mut socket, &root, None).map_err(|error| error.to_string())
    });
    let mut socket = MeasuredSocket {
        socket: TcpStream::connect(address).unwrap(),
        received: 0,
    };
    let patterns = patterns
        .iter()
        .map(|pattern| pattern.to_string())
        .collect::<Vec<_>>();
    let result = client::request_files(&mut socket, archive, output, &patterns, false, None)
        .map_err(|error| error.to_string());
    drop(socket.socket);
    let server_result = server.join().unwrap();
    if result.is_ok() {
        server_result.unwrap();
    }
    (result, socket.received)
}

#[test]
fn selects_files_from_named_archives_without_transferring_others() {
    let root = tempfile::tempdir().unwrap();
    let large = vec![b'x'; 200_000];
    archive(
        &root.path().join("first.zar"),
        &[
            ("docs/a.txt", b"first"),
            ("docs/b.md", b"markdown"),
            ("large.bin", &large),
        ],
    );
    archive(
        &root.path().join("second.zar"),
        &[("docs/a.txt", b"second"), ("root.txt", b"root")],
    );
    let out = tempfile::tempdir().unwrap();
    let (result, bytes) = request(
        root.path(),
        "first.zar",
        out.path(),
        &["*.txt", "docs/*.md"],
    );
    result.unwrap();
    assert_eq!(
        std::fs::read(out.path().join("docs/a.txt")).unwrap(),
        b"first"
    );
    assert_eq!(
        std::fs::read(out.path().join("docs/b.md")).unwrap(),
        b"markdown"
    );
    assert!(!out.path().join("large.bin").exists());
    assert!(
        bytes < 1024,
        "unselected file was transferred: {bytes} bytes"
    );
    let out2 = tempfile::tempdir().unwrap();
    request(root.path(), "second.zar", out2.path(), &["*.txt"])
        .0
        .unwrap();
    assert_eq!(
        std::fs::read(out2.path().join("docs/a.txt")).unwrap(),
        b"second"
    );
    assert_eq!(
        std::fs::read(out2.path().join("root.txt")).unwrap(),
        b"root"
    );
}

#[test]
fn reports_invalid_selections_and_refuses_overwrites() {
    let root = tempfile::tempdir().unwrap();
    archive(&root.path().join("files.zar"), &[("data.txt", b"contents")]);
    let output = tempfile::tempdir().unwrap();
    for (name, pattern, expected) in [
        ("../files.zar", "*", "'..'"),
        ("missing.zar", "*", "server:"),
        ("files.zar", "[", "invalid filter"),
        ("files.zar", "*.missing", "no regular files"),
    ] {
        let error = request(root.path(), name, output.path(), &[pattern])
            .0
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
    std::fs::write(output.path().join("data.txt"), b"keep").unwrap();
    assert!(
        request(root.path(), "files.zar", output.path(), &["*"])
            .0
            .is_err()
    );
    assert_eq!(
        std::fs::read(output.path().join("data.txt")).unwrap(),
        b"keep"
    );
}

#[test]
fn serves_case_distinct_paths_without_renaming() {
    let root = tempfile::tempdir().unwrap();
    archive(
        &root.path().join("files.zar"),
        &[
            ("name.txt", b"one"),
            ("NAME.txt", b"two"),
            ("node", b"file"),
            ("NODE/child", b"nested"),
        ],
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let path = root.path().canonicalize().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        server::serve_directory_request(&mut socket, &path, None).unwrap();
    });
    let mut socket = TcpStream::connect(address).unwrap();
    socket.write_all(b"ZARREQ02").unwrap();
    socket.write_all(&9_u32.to_be_bytes()).unwrap();
    socket.write_all(b"files.zar").unwrap();
    socket.write_all(&1_u32.to_be_bytes()).unwrap();
    socket.write_all(&1_u32.to_be_bytes()).unwrap();
    socket.write_all(b"*").unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    server.join().unwrap();
    let mut response = Cursor::new(response);
    let mut status = [0];
    response.read_exact(&mut status).unwrap();
    assert_eq!(status, [0]);
    let read_u32 = |reader: &mut Cursor<Vec<u8>>| {
        let mut bytes = [0; 4];
        reader.read_exact(&mut bytes).unwrap();
        u32::from_be_bytes(bytes)
    };
    let read_u64 = |reader: &mut Cursor<Vec<u8>>| {
        let mut bytes = [0; 8];
        reader.read_exact(&mut bytes).unwrap();
        u64::from_be_bytes(bytes)
    };
    assert_eq!(read_u32(&mut response), 4);
    assert_eq!(read_u64(&mut response), 16);
    let mut files = std::collections::BTreeMap::new();
    for _ in 0..4 {
        let path_length = read_u32(&mut response) as usize;
        let mut path = vec![0; path_length];
        response.read_exact(&mut path).unwrap();
        let size = read_u64(&mut response) as usize;
        let mut contents = vec![0; size];
        response.read_exact(&mut contents).unwrap();
        files.insert(String::from_utf8(path).unwrap(), contents);
    }
    assert_eq!(files["name.txt"], b"one");
    assert_eq!(files["NAME.txt"], b"two");
    assert_eq!(files["node"], b"file");
    assert_eq!(files["NODE/child"], b"nested");
}

#[test]
fn selects_archive_names_and_patterns_case_sensitively() {
    let root = tempfile::tempdir().unwrap();
    archive(
        &root.path().join("Files.zar"),
        &[("NAME.txt", b"upper"), ("name.txt", b"lower")],
    );
    let output = tempfile::tempdir().unwrap();
    assert!(
        request(root.path(), "files.zar", output.path(), &["*"])
            .0
            .is_err()
    );
    request(root.path(), "Files.zar", output.path(), &["NAME.txt"])
        .0
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("NAME.txt")).unwrap(),
        b"upper"
    );
}

#[test]
fn serves_large_files_without_buffering_them_in_memory() {
    let root = tempfile::tempdir().unwrap();
    let contents = vec![b'x'; 8 * 1024 * 1024 + 1];
    archive(&root.path().join("files.zar"), &[("large.bin", &contents)]);
    let output = tempfile::tempdir().unwrap();
    request(root.path(), "files.zar", output.path(), &["*"])
        .0
        .unwrap();
    assert_eq!(
        std::fs::read(output.path().join("large.bin")).unwrap(),
        contents
    );
}

#[test]
fn sends_first_file_before_preparing_the_next_and_reports_later_errors() {
    struct ServerPeer {
        request: Cursor<Vec<u8>>,
        response: Vec<u8>,
        archive: std::path::PathBuf,
        changed: bool,
    }
    impl Read for ServerPeer {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.request.read(buf)
        }
    }
    impl Write for ServerPeer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.response.extend_from_slice(buf);
            if buf == b"first" && !self.changed {
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(&self.archive)?
                    .write_all(b"changed")?;
                self.changed = true;
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct ClientPeer(Cursor<Vec<u8>>);
    impl Read for ClientPeer {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }
    impl Write for ClientPeer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let root = tempfile::tempdir().unwrap();
    let archive_path = root.path().join("files.zar");
    archive(&archive_path, &[("a.txt", b"first"), ("b.txt", b"second")]);
    let mut request = b"ZARREQ03".to_vec();
    request.extend_from_slice(&9_u32.to_be_bytes());
    request.extend_from_slice(b"files.zar");
    request.extend_from_slice(&1_u32.to_be_bytes());
    request.extend_from_slice(&1_u32.to_be_bytes());
    request.extend_from_slice(b"*");
    let mut server_peer = ServerPeer {
        request: Cursor::new(request),
        response: Vec::new(),
        archive: archive_path,
        changed: false,
    };
    let error = server::serve_directory_request(
        &mut server_peer,
        &root.path().canonicalize().unwrap(),
        None,
    )
    .unwrap_err();
    assert!(server_peer.changed);
    assert!(error.to_string().contains("archive changed"));
    let output = tempfile::tempdir().unwrap();
    let error = client::request_files(
        &mut ClientPeer(Cursor::new(server_peer.response)),
        "files.zar",
        output.path(),
        &["*".into()],
        false,
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("server: archive changed"));
    assert_eq!(
        std::fs::read(output.path().join("a.txt")).unwrap(),
        b"first"
    );
    assert!(!output.path().join("b.txt").exists());
}

#[cfg(unix)]
#[test]
fn rejects_archive_and_destination_symlink_escapes() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    archive(
        &outside.path().join("outside.zar"),
        &[("sub/file.txt", b"data")],
    );
    symlink(
        outside.path().join("outside.zar"),
        root.path().join("linked.zar"),
    )
    .unwrap();
    let output = tempfile::tempdir().unwrap();
    assert!(
        request(root.path(), "linked.zar", output.path(), &["*"])
            .0
            .unwrap_err()
            .contains("symlink")
    );
    std::fs::copy(
        outside.path().join("outside.zar"),
        root.path().join("files.zar"),
    )
    .unwrap();
    symlink(outside.path(), output.path().join("sub")).unwrap();
    assert!(
        request(root.path(), "files.zar", output.path(), &["*"])
            .0
            .is_err()
    );
    assert!(!outside.path().join("file.txt").exists());
}

#[test]
fn rejects_malformed_requests_and_responses() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        assert!(server::serve_directory_request(&mut socket, &root_path, None).is_err());
    });
    let mut socket = TcpStream::connect(address).unwrap();
    socket.write_all(b"ZARREQ01").unwrap();
    socket.write_all(&u32::MAX.to_be_bytes()).unwrap();
    socket.shutdown(Shutdown::Write).unwrap();
    let mut status = [0];
    socket.read_exact(&mut status).unwrap();
    assert_eq!(status, [1]);
    server.join().unwrap();

    let mut response = Cursor::new(vec![0, 0, 0, 0, 1]);
    response.get_mut().extend_from_slice(&0_u64.to_be_bytes());
    response.get_mut().push(0);
    response.get_mut().extend_from_slice(&9_u32.to_be_bytes());
    // A fake full-duplex peer is needed because the client writes a request before reading.
    struct Peer(Cursor<Vec<u8>>);
    impl Read for Peer {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }
    impl Write for Peer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    response.get_mut().extend_from_slice(b"../escape");
    response.get_mut().extend_from_slice(&0_u64.to_be_bytes());
    let output = tempfile::tempdir().unwrap();
    assert!(
        client::request_files(
            &mut Peer(response),
            "files.zar",
            output.path(),
            &["*".into()],
            false,
            None,
        )
        .is_err()
    );
    let mut truncated = vec![0, 0, 0, 0, 1];
    truncated.extend_from_slice(&5_u64.to_be_bytes());
    truncated.push(0);
    truncated.extend_from_slice(&8_u32.to_be_bytes());
    truncated.extend_from_slice(b"file.txt");
    truncated.extend_from_slice(&5_u64.to_be_bytes());
    truncated.extend_from_slice(b"abc");
    let error = client::request_files(
        &mut Peer(Cursor::new(truncated)),
        "files.zar",
        output.path(),
        &["*".into()],
        false,
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("incomplete transfer"));

    let mut invalid_total = vec![0, 0, 0, 0, 1];
    invalid_total.extend_from_slice(&2_u64.to_be_bytes());
    invalid_total.push(0);
    invalid_total.extend_from_slice(&8_u32.to_be_bytes());
    invalid_total.extend_from_slice(b"file.txt");
    invalid_total.extend_from_slice(&5_u64.to_be_bytes());
    let error = client::request_files(
        &mut Peer(Cursor::new(invalid_total)),
        "files.zar",
        output.path(),
        &["*".into()],
        false,
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("declared total"));
}

#[test]
fn lists_only_regular_archives_in_the_server_directory() {
    let root = tempfile::tempdir().unwrap();
    archive(&root.path().join("b.ZAR"), &[("b", b"b")]);
    archive(&root.path().join("a.zar"), &[("a", b"a")]);
    std::fs::write(root.path().join("not-zar.txt"), b"ignored").unwrap();
    std::fs::create_dir(root.path().join("directory.zar")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.path().join("a.zar"), root.path().join("link.zar")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let path = root.path().canonicalize().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut log = Vec::new();
        server::serve_directory_request(&mut socket, &path, Some(&mut log)).unwrap();
        String::from_utf8(log).unwrap()
    });
    let mut socket = TcpStream::connect(address).unwrap();
    assert_eq!(
        client::request_archives(&mut socket).unwrap(),
        ["a.zar", "b.ZAR"]
    );
    assert!(server.join().unwrap().contains("Request: list archives"));
}

#[test]
fn logs_selected_archive_and_patterns() {
    let root = tempfile::tempdir().unwrap();
    archive(&root.path().join("files.zar"), &[("data.txt", b"data")]);
    let output = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let path = root.path().canonicalize().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut log = Vec::new();
        server::serve_directory_request(&mut socket, &path, Some(&mut log)).unwrap();
        String::from_utf8(log).unwrap()
    });
    let mut socket = TcpStream::connect(address).unwrap();
    client::request_files(
        &mut socket,
        "files.zar",
        output.path(),
        &["*.txt".into()],
        false,
        None,
    )
    .unwrap();
    let log = server.join().unwrap();
    assert!(log.contains("files.zar") && log.contains("*.txt"));
}

#[test]
fn accepts_legacy_directory_request_framing() {
    let root = tempfile::tempdir().unwrap();
    archive(&root.path().join("files.zar"), &[("data.txt", b"data")]);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let path = root.path().canonicalize().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        server::serve_directory_request(&mut socket, &path, None).unwrap();
    });
    let mut socket = TcpStream::connect(address).unwrap();
    socket.write_all(b"ZARREQ01").unwrap();
    socket.write_all(&9_u32.to_be_bytes()).unwrap();
    socket.write_all(b"files.zar").unwrap();
    socket.write_all(&1_u32.to_be_bytes()).unwrap();
    socket.write_all(&1_u32.to_be_bytes()).unwrap();
    socket.write_all(b"*").unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    let mut expected = vec![0];
    expected.extend_from_slice(&1_u32.to_be_bytes());
    expected.extend_from_slice(&8_u32.to_be_bytes());
    expected.extend_from_slice(b"data.txt");
    expected.extend_from_slice(&4_u64.to_be_bytes());
    expected.extend_from_slice(b"data");
    assert_eq!(response, expected);
    server.join().unwrap();
}

#[test]
fn force_overwrites_regular_files_and_progress_can_be_disabled() {
    let root = tempfile::tempdir().unwrap();
    archive(
        &root.path().join("files.zar"),
        &[("data.txt", b"new contents")],
    );
    let output = tempfile::tempdir().unwrap();
    std::fs::write(output.path().join("data.txt"), b"old contents").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let path = root.path().canonicalize().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        server::serve_directory_request(&mut socket, &path, None).unwrap();
    });
    let mut socket = TcpStream::connect(address).unwrap();
    let progress = indicatif::ProgressBar::hidden();
    client::request_files(
        &mut socket,
        "files.zar",
        output.path(),
        &["*".into()],
        true,
        Some(&progress),
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(
        std::fs::read(output.path().join("data.txt")).unwrap(),
        b"new contents"
    );
    assert_eq!(progress.length(), Some(12));
    assert_eq!(progress.position(), 12);
    assert!(progress.is_finished());
}

#[cfg(unix)]
#[test]
fn force_does_not_follow_output_symlinks() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    archive(&root.path().join("files.zar"), &[("data.txt", b"new")]);
    let output = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("data.txt"), b"keep").unwrap();
    symlink(
        outside.path().join("data.txt"),
        output.path().join("data.txt"),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let path = root.path().canonicalize().unwrap();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let _ = server::serve_directory_request(&mut socket, &path, None);
    });
    let mut socket = TcpStream::connect(address).unwrap();
    assert!(
        client::request_files(
            &mut socket,
            "files.zar",
            output.path(),
            &["*".into()],
            true,
            None
        )
        .is_err()
    );
    drop(socket);
    server.join().unwrap();
    assert_eq!(
        std::fs::read(outside.path().join("data.txt")).unwrap(),
        b"keep"
    );
}

#[test]
fn updates_progress_during_a_transfer() {
    struct SlowPeer {
        response: Cursor<Vec<u8>>,
        progress: indicatif::ProgressBar,
    }
    impl Read for SlowPeer {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.response.position() == 35 {
                assert_eq!(self.progress.position(), 1);
            }
            if self.response.position() >= 34 {
                self.response.read(&mut buf[..1])
            } else {
                self.response.read(buf)
            }
        }
    }
    impl Write for SlowPeer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut response = vec![0, 0, 0, 0, 1];
    response.extend_from_slice(&2_u64.to_be_bytes());
    response.push(0);
    response.extend_from_slice(&8_u32.to_be_bytes());
    response.extend_from_slice(b"file.txt");
    response.extend_from_slice(&2_u64.to_be_bytes());
    response.extend_from_slice(b"xy");
    let progress = indicatif::ProgressBar::hidden();
    let mut peer = SlowPeer {
        response: Cursor::new(response),
        progress: progress.clone(),
    };
    let output = tempfile::tempdir().unwrap();
    client::request_files(
        &mut peer,
        "files.zar",
        output.path(),
        &["*".into()],
        false,
        Some(&progress),
    )
    .unwrap();
    assert_eq!(progress.length(), Some(2));
    assert_eq!(progress.position(), 2);
    assert!(progress.is_finished());
}
