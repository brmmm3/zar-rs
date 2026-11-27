# AGENTS.md (zarmcp - Model Context Protocol Server)

`zarmcp` is an MCP (Model Context Protocol) server running over standard I/O (stdio). It exposes ZAR archive operations as structured tool calls to AI agents and LLM clients.

## Build & Test Instructions
- **Build**: `cargo build --bin zarmcp` (or `--release`)
- **Run tests**: `cargo test -p zarmcp`
- **Lint**: `cargo clippy -p zarmcp`
- **Format**: `cargo fmt`

## Server Tools

### Read-Only Tools
- `info_archive(archive)`: Inspects archive structure, commit history, metadata, and encryption flags.
- `list_archive(archive, password?, passwords?, to?)`: Lists all files, folders, and attributes in the archive at a specific commit.
- `check_archive(archive, password?, passwords?)`: Validates block checksums and authentication tags.
- `read_file(archive, path, password?, passwords?, to?)`: Reads file contents directly into memory (capped at 1 MB; returns base64 data for binary files or UTF-8 text).
- `grep_archive(archive, pattern, password?, passwords?, to?)`: Searches archive contents using regular expressions without disk extraction.

### Mutating Tools
- `create_archive(archive, paths, password?, aead?, encryption_scope?, strict?)`: Creates a new archive from files, folders, or imported formats.
- `add_to_archive(archive, paths, password?, passwords?, aead?, encryption_scope?, strict?)`: Appends a new commit with added or updated paths.
- `remove_from_archive(archive, paths, password?, passwords?, removal_password?)`: Records path removals in a new commit while preserving history.
- `extract_archive(archive, destination, password?, passwords?, to?, dry_run?)`: Extracts archive files to a destination directory.

## CLI Execution & Constraints
- **Stdio transport**: The server communicates via JSON-RPC over `stdin` and `stdout`. Do not emit non-protocol log messages to `stdout`.
- **Read-Only Mode**: Pass `--read-only` to only expose non-mutating inspection and reading tools.
- **Allowed Tools Filter**: Pass `--allow-tools <tool1,tool2>` to restrict exposed capabilities.
- **Interactive Debugging**:
  ```sh
  npx @modelcontextprotocol/inspector cargo run --bin zarmcp
  ```

## Client Configuration (JSON)
For integration with Claude Desktop, Cursor, Gemini CLI, or other MCP hosts:

```json
{
  "mcpServers": {
    "zarmcp": {
      "command": "cargo",
      "args": ["run", "--release", "--bin", "zarmcp"],
      "env": {}
    }
  }
}
```
