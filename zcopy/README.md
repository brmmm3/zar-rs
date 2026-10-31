# zcopy

Copies a file tree from one machine to another over TCP, compressed and
optionally encrypted, using the `zarlib` create/extract pipelines. Only files
that are missing on the destination or newer on the source are sent.

```sh
# destination machine (folder is created if missing)
zcopy -s /data/backup -p secret --allow laptop,10.0.0.5

# source machine
zcopy -c server.example.com ~/projects -p secret
```

Sending a folder `foo` creates `foo` inside the server folder and writes its
contents there (`zcopy -c host ~/projects/foo` gives `/data/backup/foo/...`). Sending
`.` or a filesystem root, which has no name, writes directly into the server folder.

The client pushes: `-s` names the folder that receives files, `-c` names the
server and the optional positional `SOURCE` (default `.`) is the tree to send.

## How it works

1. At startup the server scans its folder with `scandir` and caches path, size,
   mtime and kind.
2. On connect the server sends that metadata zstd-compressed.
3. The client scans its source (symlinks and special files are skipped) and
   sends a file when it is missing remotely or its mtime is newer. Empty
   directories missing remotely are sent too.
4. Files travel in batches as `zarlib` archives (zstd-compressed, encrypted when a
   password is set). The server extracts each batch, restores mtimes, and updates
   its cache.

A second run therefore sends nothing unless something changed. Files deleted on
the source are not deleted on the destination.

## Progress and concurrent clients

The client shows a progress bar like `zarc` on stderr. It tracks what has actually
been sent over the network: the bar and ETA are scaled to the source size, and the
text beside it shows the bytes sent on the wire and the current wire speed. The server accepts several clients at once. When a
client connects while others are connected, it prints a warning and asks
`Continue anyway? [y/N]`; without a terminal it cancels unless `--yes` is given.
Extraction on the server is serialised, so concurrent clients are safe but share
one extraction pipeline.

## Server log

The server prints one line per event: a client connecting (with the source
folder it sends and the destination folder), each received batch (file count,
transferred size, and top-level names copied), and the disconnect with the file count,
received size, duration and average speed (for example `2 file(s), 190.74 MiB
received in 1.3s (146.24 MiB/s)`), or the error that ended the connection.

## Options

| Option | Meaning |
|---|---|
| `-s, --server <FOLDER>` | Run as server |
| `-c, --client <HOST[:PORT]>` | Run as client |
| `[SOURCE]` | Folder to send (client), default `.` |
| `-p, --password` | Password for authentication and encryption; also `ZCOPY_PASSWORD` |
| `--aead` | `aes-256-gcm` or `chacha20-poly1305` (client defaults to the server's) |
| `-l, --level` | zstd compression level (client defaults to the server's) |
| `--allow a,b,c` | Machine names or IPs allowed to connect (server) |
| `--port`, `--bind` | TCP port (default 7878) and listen address (default `0.0.0.0`) |
| `--batch-mib` | Approximate batch size, default 512 |
| `--metadata` | Also transfer permissions and ownership; the server must enable it too (`--metadata` or `"metadata": true`) |
| `-y, --yes` | Continue without asking when other clients are connected (client) |
| `--no-progress` | Hide the progress bar (client) |
| `--temp <DIR>` | Directory for temporary archives (client) |
| `--config <FILE>` | Settings file, default `zcopy.json` if present |

## zcopy.json

All of `password`, `aead`, `level`, `allow`, `port`, `bind` and `batch_mib` and `metadata` can be
set in a JSON file; command-line arguments take precedence. Unknown keys are an
error.

```json
{ "password": "secret", "aead": "chacha20-poly1305", "level": 5,
  "allow": ["laptop", "10.0.0.5"], "port": 7878 }
```

The password is stored in plain text, so protect the file's permissions.

## Security notes

- Metadata transfer is opt-in on the server because a client could otherwise
  choose permissions (including setuid bits) and ownership of files it writes,
  which matters when the server runs as root. Only enable it for trusted clients.
  Without it, only modification times are transferred.
- With a password, the connection is authenticated and encrypted (Argon2id key,
  AES-256-GCM frames) and file data is encrypted by `zarlib`. Without a password
  nothing is encrypted or authenticated; combine at least an allow list with a
  trusted network. The server warns when neither is configured.
- The allow list compares the peer IP with the resolved addresses of each entry,
  so it is only as trustworthy as your network and DNS.
- The server rejects absolute, `..` and backslash paths from clients, refuses paths that pass through a symlink, and relies on `zarlib`'s canonical-path and no-follow extraction, so nothing outside the server folder is written or modified.
- The server uses `<folder>/.zcopy-tmp` for incoming archives; that name is
  never synchronised.
