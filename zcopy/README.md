# zcopy

`zcopy` pushes a directory tree from a client to a server over TCP. It uses
ZAR's zstd compression and extraction pipelines and can authenticate and
encrypt transfers with a shared password. In its default mode it copies only
missing or newer files; alternatively, it saves the complete tree as a `.zar`
archive on the server.

Typical uses include copying projects to another machine, updating a remote
backup directory without retransmitting unchanged files, collecting files from
multiple clients, and sending a compressed archive for later extraction.
`zcopy` is a one-way copy tool, not a bidirectional synchronizer or a
point-in-time backup system.

## Install

Install a Rust toolchain supporting the 2024 edition. From the repository root:

```sh
cargo build --release -p zcopy
```

The executable is `target/release/zcopy` (`zcopy.exe` on Windows). Alternatively,
install it into Cargo's binary directory with `cargo install --path zcopy`.
Install the executable on both machines and make the server's TCP port
reachable from the client.

## Quick start

On the receiving machine, start a server. Set the same `ZCOPY_PASSWORD` in both
environments before running these commands; use a strong password and protect
it as a secret.

```sh
# Server: receive into /data/backup and allow only these client addresses.
zcopy --server /data/backup --allow laptop,10.0.0.5

# Client: send ~/projects to the server.
zcopy --client server.example.com ~/projects
```

The server creates its destination folder if needed and continues listening
until stopped. The example creates `/data/backup/projects/...`; the client
prints transfer statistics and exits. Run the client again to copy missing or
newer files. Both sides must either use the same password or both omit it.
Without a password, the connection is neither encrypted nor authenticated.

To select a non-default port, use `--port` on both sides, or specify the port
in the client's address:

```sh
zcopy -s /data/backup --bind 192.0.2.10 --port 9000
zcopy -c server.example.com:9000 ~/projects
```

IPv6 addresses are supported; use brackets when specifying a port, for example
`-c '[2001:db8::10]:9000'`. `--bind` defaults to `0.0.0.0` and the port
defaults to `7878`.

## Copy and update files

The client sends a **folder**, not an individual file. `SOURCE` defaults to
`.`. The source is resolved to an absolute path, and a named source folder is
created under the server folder. Thus, if the current directory is
`/home/alex/projects`, `zcopy -c host .` sends it as `projects/...`, not
directly into the destination root. A filesystem root has no folder name, so
its contents are placed directly in the server folder.

For an unpacked transfer:

1. The server scans its destination at startup and caches paths, sizes,
   modification times and entry types. It sends a compressed manifest to each
   client.
2. The client scans the source and selects regular files that are missing on
   the destination or have a newer source modification time. Missing empty
   directories are also included; symlinks and special files are skipped.
3. Selected entries are compressed into ZAR batches, transmitted and extracted
   on the server. Existing selected files are overwritten, file modification
   times are restored, and the server updates its cache.

Comparison uses modification times in **whole seconds**, not file contents or
size. A changed file whose timestamp is equal to or older than the server's
cached timestamp is skipped, even if its contents differ. Keep machine clocks
and timestamps reliable. Deletions on the source are not propagated, and
destination-only files remain in place.

The cache is refreshed by zcopy transfers, not by watching external changes to
the destination. Restart the server to rescan after files are changed outside
zcopy. Concurrent clients are warned about each other, but their manifests
are snapshots; coordinate overlapping writes if their ordering matters.

## Save a packed archive

Use `--pack` to send the complete source tree as one archive rather than
extracting it on receipt:

```sh
zcopy -c server.example.com --pack ~/projects
```

This writes `/data/backup/projects.zar` in the quick-start setup and replaces
an existing file with the same archive name. Packed transfers do not use the
destination manifest to select changed files and do not use the batch size
limit: each run creates and sends a complete archive. A named empty source
folder is included too. A filesystem root uses the fallback name
`received.zar`. Regular files and directories are included; symlinks and
special files are skipped.

The archive is encrypted when a password is configured. Keep that password
for later extraction. For example, on the receiving machine:

```sh
zarc extract /data/backup/projects.zar --output /data/restored
```

Use the password option supported by `zarc` when extracting an encrypted
archive; see the [zarc documentation](../zarlib-cli/README.md). Packed archive
names can collide when different source folders have the same name.
`--unpack` explicitly selects the default extracted-file mode and cannot be
combined with `--pack`.

## Permissions, timestamps and temporary storage

File modification times are transferred separately and restored in unpack
mode. Other metadata is opt-in: enable `--metadata` on **both** client and
server to transfer and restore permissions and ownership where supported.
If the client requests metadata but the server has not enabled it, zcopy warns
and continues without it. Metadata restoration depends on the receiver's
platform and privileges; it is not a guarantee of identical ownership on a
different machine.

The client builds temporary archives in a generated directory under the system
temporary directory. `--temp DIR` chooses the directory instead. **That entire
directory is recursively removed after the transfer**, including any files
already in it. Use a dedicated, disposable path, never a shared directory or a
directory containing important data.

The server uses `<destination>/.zcopy-tmp` for incoming archives. It removes
and recreates this directory at startup; its contents are not synchronized.
Reserve it exclusively for zcopy. Both machines need enough temporary disk
space for their in-flight archives; a packed transfer can require space for
the complete compressed archive.

## Progress, concurrency and automation

The client progress bar is displayed on stderr. It estimates progress and ETA
in source bytes while reporting archive bytes sent and average wire speed.
The final summary reports transferred entries, source bytes, archive bytes,
up-to-date files and elapsed time. `--no-progress` hides the bar.

The server accepts multiple clients and logs connections, batches, destination
paths, transfer sizes, disconnects and errors. Extraction is serialized.
When another client is connected, a new client warns and asks whether to
continue. Without an interactive terminal it cancels unless `--yes` is set:

```sh
zcopy -c server.example.com ~/projects --yes --no-progress
```

For scheduled jobs, provide the password through a protected environment or
configuration file, check the exit status, and arrange appropriate logging.
Transfers are made in batches and are not an atomic snapshot of a changing
source tree; a failed or interrupted transfer can leave earlier batches on the
server. Check errors and rerun as appropriate.

## Options

Run `zcopy --help` for the CLI's current usage.

| Option | Meaning |
|---|---|
| `-s, --server <FOLDER>` | Serve and receive into this folder. Mutually exclusive with `--client`. |
| `-c, --client <HOST[:PORT]>` | Push to a server by hostname or IP address. |
| `[SOURCE]` | Client source folder; defaults to `.`. |
| `--pack` | Save the complete source as a `.zar` file on the server. |
| `--unpack` | Extract batches on the server; the default. |
| `-p, --password <PASSWORD>` | Shared password; alternatively set `ZCOPY_PASSWORD`. |
| `--aead <AEAD>` | Archive encryption: `aes-256-gcm` or `chacha20-poly1305`. Client defaults to the server setting; server defaults to AES-256-GCM. |
| `-l, --level <LEVEL>` | zstd compression level. Client defaults to the server setting; server defaults to `3`. |
| `--allow <HOSTS>` | Server allow list of comma-separated IP addresses or machine names. |
| `--port <PORT>` | Server listen port or client default port; default `7878`. A port in `HOST:PORT` takes precedence for that connection. |
| `--bind <ADDRESS>` | Server listen address; default `0.0.0.0`. |
| `--batch-mib <MIB>` | Approximate client batch size in MiB; default `512`, minimum `1`. A batch can exceed the limit for a large file. |
| `--metadata` | Client sends permissions and ownership; server allows them to be applied. Both must enable it. |
| `-y, --yes` | Continue when other clients are connected without prompting. |
| `--no-progress` | Hide the client progress bar. |
| `--temp <DIR>` | Client temporary directory; the entire directory is removed after use. |
| `--config <FILE>` | Read settings from this JSON file. |
| `-h, --help` | Show help. |
| `-V, --version` | Show version. |

## Configuration file

By default, zcopy reads `zcopy.json` from its current working directory if it
exists. `--config FILE` selects a file explicitly and fails if it cannot be
read. Unknown keys are errors. The supported settings are `password`, `aead`,
`level`, `allow`, `port`, `bind`, `batch_mib` and `metadata`:

```json
{
  "aead": "chacha20-poly1305",
  "level": 5,
  "allow": ["laptop", "10.0.0.5"],
  "port": 7878,
  "bind": "0.0.0.0",
  "batch_mib": 512,
  "metadata": false
}
```

Command-line settings override the file. For the password, `--password` takes
precedence over `ZCOPY_PASSWORD`, which takes precedence over the file. A
password in JSON is stored in plaintext; restrict file permissions. Command
arguments and environment variables can also be exposed to other processes or
logs, depending on the system.

## Security

- With a password, zcopy derives a key using Argon2id and authenticates and
  encrypts protocol frames with AES-256-GCM. Archive contents are separately
  encrypted using the selected AEAD. The server and client must share the
  password. Without one, neither the control messages nor archives are
  protected; use a trusted network and an allow list at minimum.
- An allow list compares the peer IP address with each literal IP or resolved
  hostname. It is not authentication and depends on the network and DNS.
  Use firewall rules or a private network as an additional boundary.
- Only enable server-side metadata restoration for trusted clients: file
  permissions and ownership, including privileged mode bits, can have security
  consequences, especially when the server runs with elevated privileges.
- Extraction rejects unsafe paths and refuses symlink traversal. The server
  also uses zarlib's safe extraction mechanisms. Restrict access to the
  destination and temporary directories, and avoid running the server with
  more privileges than it needs.
