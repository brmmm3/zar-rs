use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn new() -> Self {
        Self::with_args(&[])
    }

    fn with_args(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zarmcp"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut client = Self {
            child,
            input: Some(input),
            output,
            next_id: 0,
        };
        let response = client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "integration-test", "version": "1"}
            }),
        );
        assert!(response["result"]["capabilities"]["tools"].is_object());
        client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        client
    }
}

#[test]
fn read_only_and_allowlist_block_undiscovered_tools() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("archive.zar");
    let source = dir.path().join("file.txt");
    let output = dir.path().join("output");
    std::fs::write(&source, b"example").unwrap();
    let mut setup = Client::new();
    setup.success(
        "create_archive",
        json!({"archive": archive, "paths": [source]}),
    );
    drop(setup);
    let original = std::fs::read(&archive).unwrap();

    let mut client = Client::with_args(&["--read-only"]);
    let tools = client.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 5);
    for name in [
        "info_archive",
        "list_archive",
        "check_archive",
        "read_file",
        "grep_archive",
    ] {
        assert!(tools.iter().any(|tool| tool["name"] == name));
    }
    let read = client.success("read_file", json!({"archive": archive, "path": "file.txt"}));
    assert_eq!(
        STANDARD.decode(read["data"].as_str().unwrap()).unwrap(),
        b"example"
    );
    for (name, args) in [
        (
            "create_archive",
            json!({"archive": output, "paths": [source]}),
        ),
        (
            "add_archive",
            json!({"archive": archive, "paths": [source]}),
        ),
        (
            "remove_archive",
            json!({"archive": archive, "paths": ["file.txt"]}),
        ),
        (
            "extract_archive",
            json!({"archive": archive, "destination": output}),
        ),
    ] {
        assert!(client.call(name, args)["error"].is_object(), "{name}");
    }
    assert_eq!(std::fs::read(&archive).unwrap(), original);
    assert!(!output.exists());

    let mut limited = Client::with_args(&[
        "--read-only",
        "--allow-tools",
        "info_archive",
        "--allow-tools",
        "read_file",
    ]);
    let tools = limited.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().any(|tool| tool["name"] == "info_archive"));
    assert!(tools.iter().any(|tool| tool["name"] == "read_file"));
    assert!(limited.call("list_archive", json!({"archive": archive}))["error"].is_object());
    limited.success("info_archive", json!({"archive": archive}));

    let mut write_only = Client::with_args(&["--allow-tools", "create_archive"]);
    assert!(write_only.call("info_archive", json!({"archive": archive}))["error"].is_object());
    let new_archive = dir.path().join("new.zar");
    write_only.success(
        "create_archive",
        json!({"archive": new_archive, "paths": [source]}),
    );
    assert!(new_archive.exists());
}

#[test]
fn invalid_policy_fails_before_initialization() {
    for args in [
        vec!["--allow-tools", "not_a_tool"],
        vec!["--read-only", "--allow-tools", "create_archive"],
        vec!["--allow-tools", ""],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_zarmcp"))
            .args(&args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
}

impl Client {
    fn send(&mut self, message: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut line = String::new();
        assert_ne!(
            self.output.read_line(&mut line).unwrap(),
            0,
            "server exited"
        );
        let response: Value =
            serde_json::from_str(&line).expect("stdout must contain only JSON-RPC");
        assert_eq!(response["id"], id);
        response
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
    }

    fn success(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.call(name, arguments);
        assert_ne!(response["result"]["isError"], true, "{response}");
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    }

    fn error(&mut self, name: &str, arguments: Value) -> String {
        let response = self.call(name, arguments);
        assert_eq!(response["result"]["isError"], true, "{response}");
        response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        drop(self.input.take());
        let _ = self.child.wait();
    }
}

#[test]
fn stdio_discovery_and_archive_history() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("history.zar");
    let source = dir.path().join("file[1].txt");
    let restored = dir.path().join("restored");
    std::fs::write(&source, b"old bytes").unwrap();
    let mut client = Client::new();
    let tools = client.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    for name in [
        "create_archive",
        "add_archive",
        "remove_archive",
        "info_archive",
        "list_archive",
        "check_archive",
        "extract_archive",
        "read_file",
    ] {
        assert!(tools.iter().any(|tool| tool["name"] == name), "{name}");
    }
    client.success(
        "create_archive",
        json!({"archive": archive, "paths": [source]}),
    );
    std::fs::write(&source, b"new bytes").unwrap();
    client.success(
        "add_archive",
        json!({"archive": archive, "paths": [source]}),
    );
    let info = client.success("info_archive", json!({"archive": archive}));
    assert_eq!(info.as_array().unwrap().len(), 2);
    let listing = client.success("list_archive", json!({"archive": archive}));
    assert!(
        listing
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "file[1].txt")
    );
    let read = client.success(
        "read_file",
        json!({"archive": archive, "path": "file[1].txt", "to": "00000001", "offset": 4, "size_limit": 3}),
    );
    assert_eq!(
        STANDARD.decode(read["data"].as_str().unwrap()).unwrap(),
        b"byt"
    );
    assert!(
        client
            .error("read_file", json!({"archive": archive, "path": "missing"}))
            .contains("not found")
    );
    assert!(
        client
            .error(
                "read_file",
                json!({"archive": archive, "path": "file[1].txt", "size_limit": 1048577})
            )
            .contains("size_limit")
    );
    client.success(
        "remove_archive",
        json!({"archive": archive, "paths": ["file[1].txt"]}),
    );
    assert_eq!(
        client
            .success("list_archive", json!({"archive": archive}))
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        client.success("check_archive", json!({"archive": archive}))["commits"],
        3
    );
    client.success(
        "extract_archive",
        json!({"archive": archive, "destination": restored, "to": "00000001"}),
    );
    assert_eq!(
        std::fs::read(restored.join("file[1].txt")).unwrap(),
        b"old bytes"
    );
}

#[test]
fn encrypted_operations_require_and_authenticate_passwords() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("encrypted.zar");
    let source = dir.path().join("secret.txt");
    std::fs::write(&source, b"secret").unwrap();
    let mut client = Client::new();
    client.success(
        "create_archive",
        json!({"archive": archive, "paths": [source], "password": "correct"}),
    );
    assert!(
        client
            .error("list_archive", json!({"archive": archive}))
            .contains("password required")
    );
    client.error(
        "list_archive",
        json!({"archive": archive, "password": "incorrect"}),
    );
    let read = client.success(
        "read_file",
        json!({"archive": archive, "path": "secret.txt", "password": "correct"}),
    );
    assert_eq!(
        STANDARD.decode(read["data"].as_str().unwrap()).unwrap(),
        b"secret"
    );
    client.success(
        "check_archive",
        json!({"archive": archive, "password": "correct"}),
    );
    std::fs::write(&source, b"updated").unwrap();
    assert!(
        client
            .error(
                "add_archive",
                json!({"archive": archive, "paths": [source], "passwords": {"00000001": "correct"}})
            )
            .contains("password is required")
    );
    client.success("add_archive", json!({"archive": archive, "paths": [source], "password": "new", "passwords": {"00000001": "correct"}}));
    client.error(
        "list_archive",
        json!({"archive": archive, "password": "new"}),
    );
    let credentials = json!({"00000001": "correct", "00000002": "new"});
    let read = client.success(
        "read_file",
        json!({"archive": archive, "path": "secret.txt", "passwords": credentials}),
    );
    assert_eq!(
        STANDARD.decode(read["data"].as_str().unwrap()).unwrap(),
        b"updated"
    );
    assert!(
        client
            .error(
                "remove_archive",
                json!({"archive": archive, "paths": ["secret.txt"], "passwords": credentials})
            )
            .contains("removal_password")
    );
    client.success("remove_archive", json!({"archive": archive, "paths": ["secret.txt"], "passwords": credentials, "removal_password": "new"}));
    assert_eq!(
        client
            .success(
                "list_archive",
                json!({"archive": archive, "password": "new", "passwords": {"00000001": "correct"}})
            )
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert!(client.success("list_archive", json!({"archive": archive, "password": "new", "passwords": {"00000001": "correct"}, "to": "00000001"})).as_array().unwrap().iter().any(|entry| entry["path"] == "secret.txt"));
}

#[test]
fn grep_archive_finds_patterns_and_respects_options_and_passwords() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("archive.zar");
    let file1 = dir.path().join("main.rs");
    let file2 = dir.path().join("notes.txt");
    std::fs::write(&file1, b"fn main() {\n    println!(\"hello world\");\n}\n").unwrap();
    std::fs::write(&file2, b"first line\nhello from notes\nthird line\n").unwrap();

    let mut client = Client::new();
    client.success(
        "create_archive",
        json!({"archive": archive, "paths": [file1, file2]}),
    );

    // Default search across all files (file_filter omitted)
    let result = client.success(
        "grep_archive",
        json!({"archive": archive, "pattern": "hello"}),
    );
    let files = result.as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["path"], "main.rs");
    assert_eq!(files[0]["lines"][0]["line_number"], 2);
    assert_eq!(files[0]["lines"][0]["is_match"], true);
    assert_eq!(
        files[0]["lines"][0]["line"],
        "    println!(\"hello world\");"
    );
    assert_eq!(files[1]["path"], "notes.txt");
    assert_eq!(files[1]["lines"][0]["line_number"], 2);
    assert_eq!(files[1]["lines"][0]["is_match"], true);
    assert_eq!(files[1]["lines"][0]["line"], "hello from notes");

    // Filter by file_filter glob
    let result = client.success(
        "grep_archive",
        json!({"archive": archive, "pattern": "hello", "file_filter": "*.rs"}),
    );
    let files = result.as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "main.rs");

    // Context lines: before = 1, after = 1
    let result = client.success(
        "grep_archive",
        json!({
            "archive": archive,
            "pattern": "hello from",
            "file_filter": "*.txt",
            "before": 1,
            "after": 1
        }),
    );
    let files = result.as_array().unwrap();
    assert_eq!(files.len(), 1);
    let lines = files[0]["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["line_number"], 1);
    assert_eq!(lines[0]["line"], "first line");
    assert_eq!(lines[0]["is_match"], false);
    assert_eq!(lines[1]["line_number"], 2);
    assert_eq!(lines[1]["line"], "hello from notes");
    assert_eq!(lines[1]["is_match"], true);
    assert_eq!(lines[2]["line_number"], 3);
    assert_eq!(lines[2]["line"], "third line");
    assert_eq!(lines[2]["is_match"], false);

    // Lookaround regex
    let result = client.success(
        "grep_archive",
        json!({"archive": archive, "pattern": "(?<=println!\\(\")hello"}),
    );
    let files = result.as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "main.rs");

    // Encrypted archive
    let enc_archive = dir.path().join("encrypted.zar");
    client.success(
        "create_archive",
        json!({"archive": enc_archive, "paths": [file1], "password": "secret_pass"}),
    );
    // Missing password fails
    assert!(
        client
            .error(
                "grep_archive",
                json!({"archive": enc_archive, "pattern": "hello"})
            )
            .contains("password required")
    );
    // Correct password succeeds
    let result = client.success(
        "grep_archive",
        json!({"archive": enc_archive, "pattern": "hello", "password": "secret_pass"}),
    );
    let files = result.as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]["lines"][0]["line"],
        "    println!(\"hello world\");"
    );

    // Works with --read-only and --allow-tools grep_archive
    let mut restricted = Client::with_args(&["--allow-tools", "grep_archive"]);
    let result = restricted.success(
        "grep_archive",
        json!({"archive": archive, "pattern": "main"}),
    );
    assert_eq!(result.as_array().unwrap().len(), 1);
}
