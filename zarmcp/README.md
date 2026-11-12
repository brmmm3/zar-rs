# zarmcp

`zarmcp` is a local [Model Context Protocol](https://modelcontextprotocol.io/)
server that lets an MCP client inspect and manage ZAR archives through
`zarlib`. It communicates using stdio; it does not start a network service.

## Build and configure

From the repository root, build a release binary:

```sh
cargo build --release -p zarmcp
```

Configure your MCP client to launch the resulting executable. For example, a
client using an `mcpServers` configuration can use:

```json
{
  "mcpServers": {
    "zar": {
      "command": "/absolute/path/to/zar-rs/target/release/zarmcp",
      "args": []
    }
  }
}
```

Replace the path with the actual executable path (`zarmcp.exe` on Windows).
Configuration formats vary by client. Relative paths in tool requests are
resolved against the server process's working directory; absolute paths avoid
depending on the client's launch directory. Standard output is reserved for
MCP messages.

### Restrict available tools

By default all tools below are available. Configure the server's startup
arguments in the MCP client to restrict both tool discovery and tool calls:

- `--read-only` enables only `info_archive`, `list_archive`, `check_archive`,
  and `read_file`. It disables `extract_archive` as well as archive
  modifications, including for extraction dry runs.
- `--allow-tools TOOL,...` enables only the named tools. It can be repeated.
  For example, `--allow-tools info_archive,read_file` exposes those two tools
  and nothing else.
- Combine the options to allow a subset of read-only tools. Unknown names,
  empty lists, or a write-capable tool in an allowlist combined with
  `--read-only` make startup fail instead of silently weakening the policy.

For example:

```json
{
  "mcpServers": {
    "zar": {
      "command": "/absolute/path/to/zar-rs/target/release/zarmcp",
      "args": ["--read-only", "--allow-tools", "info_archive,list_archive,read_file"]
    }
  }
}
```

Disabled tools are not listed and direct calls to them are rejected. The
policy is fixed at startup; change the configuration and restart the server to
change it. These options restrict this server's tools, not other tools the AI
client may have. Read-only mode still allows reading any archive accessible to
the server process; it is not a filesystem sandbox and reads may update
filesystem access times.

## Tools

| Tool | Purpose |
|---|---|
| `create_archive` | Create a new archive from local files, directories, or supported archive inputs. Existing archives are not overwritten. |
| `add_archive` | Append new or updated files as a commit. It does not remove files deleted from disk. |
| `remove_archive` | Append removals of archive-relative paths; previous versions remain in history. |
| `info_archive` | Inspect commit IDs and encryption information. |
| `list_archive` | List the latest state or a historical state selected by `to`. |
| `check_archive` | Verify archive framing, checksums, and encrypted content. |
| `extract_archive` | Extract to a local destination, optionally selecting a historical commit or using `dry_run`. Existing files are not overwritten. |
| `read_file` | Read a range of one regular file from the archive without extracting it to disk. |

Tool arguments and descriptions are available through MCP tool discovery.
Successful results are JSON in a text content block; operation failures are
MCP tool errors. Create and add results include library warnings: inspect them
because inputs can be skipped. All operations in a server process are
serialized; this does not coordinate with other processes accessing an archive.
`join`, recovery scanning, and other advanced operations remain available in
the [CLI](../zarlib-cli/README.md), but are not exposed by this server.

`read_file` returns base64-encoded bytes, an offset, and the number of bytes
returned. `size_limit` defaults to 1 MiB and cannot exceed 1 MiB. A full-sized
response may have more data; use the next byte offset to read another range.
This supports binary files as well as text. A partial read is not a full-file
checksum verification; use `check_archive` to verify the archive.

### Encrypted archives

`password` supplies a default password; `passwords` maps eight-digit commit IDs
to passwords and overrides the default for those commits. `info_archive`
reports commit IDs without credentials. Other operations require passwords for
**all** encrypted commits, even when selecting an earlier state, so they do not
silently return a partial view. A wrong password for a commit that the
operation reads causes it to fail; selecting an earlier state does not
authenticate later commits. Use `check_archive` to check the whole archive.

`create_archive` encrypts its new commit when `password` is supplied.
`add_archive` uses `password` for the new commit and as the default for
existing commits. When adding to an encrypted archive, `password` is required
so the new commit is not accidentally left unencrypted. Use `passwords` to
unlock existing commits that use different passwords. `aead` accepts
`aes-256-gcm` or `chacha20-poly1305`; `encryption_scope` accepts `data`,
`header`, or `header-and-data`. Defaults are AES-256-GCM and header-and-data.

For encrypted archives, `remove_archive` encrypts the new removal commit with
`removal_password`, or with `password` if `removal_password` is omitted. If
only per-commit passwords are provided, specify `removal_password` separately.
Removal commits use AES-256-GCM and header-and-data encryption.

## Security and operational boundaries

- **Trust the client.** The server has the filesystem permissions of the user
  running it; it does not confine paths to a workspace or request confirmation.
  Tools can read local files, create archives, modify archive history, and write
  extracted files. Enable it only for trusted local clients and use client-side
  approval controls where available.
- **Passwords are tool arguments.** They can be visible to the MCP client,
  model provider, conversation history, and client logs. Do not send sensitive
  credentials to a client or service you do not trust; prefer unencrypted test
  archives when appropriate. The server does not intentionally log requests.
- **Archive content is untrusted.** File names and contents can contain
  instructions aimed at an AI assistant. Treat them as data, not instructions,
  and review proposed tool calls before allowing writes.
- `extract_archive` uses the library's default extraction safeguards and does
  not enable symlink or device restoration. It does not restore timestamps;
  see the [project limitations](../README.md#limitations).
- Keep separate backups before modifying an archive, and verify and test
  restores. This server is not a replacement for a backup policy.

## Test

```sh
cargo test --locked -p zarmcp
```

The integration tests launch the server over stdio and exercise tool
discovery, history, extraction, errors, and encrypted commits.
