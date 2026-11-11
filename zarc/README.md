# `zarc` command-line guide

`zarc` creates and manages ZAR archives. Build it from the workspace root with
`cargo build -p zarc --release`; run `cargo run -p zarc -- <command>` during
development, or install it with `cargo install --path zarc`.
For an alternate release allocator, build with `--features mimalloc`.

Get the full command list with:

```sh
zarc --help
zarc <command> --help
```

One-letter shortcuts are shown in the table.

| Command | Shortcut |
|---|---|
| `create` | `c` |
| `add` | `a` |
| `remove` | `r` |
| `join` | `j` |
| `remove-commits` | - |
| `convert` | - |
| `meta` | `m` |
| `encrypt` | - |
| `decrypt` | - |
| `check` | `k` |
| `info` | `i` |
| `list` | `l` |
| `extract` | `e` |

For example, `zarc a backup.zar new-file.txt` is equivalent to
`zarc add backup.zar new-file.txt`. `check` uses `k` to keep `c` available for
`create`.

## Common workflows

### Create an archive

```sh
zarc create backup.zar docs/ src/ report.txt
zarc create converted.zar downloaded.zip
zarc create converted-7z.zar downloaded.7z
zarc create converted-rar.zar downloaded.rar
zarc create converted-iso.zar disc.iso
zarc create converted-tar.zar downloaded.tar
zarc create converted-tar-gz.zar downloaded.tar.gz
zarc create converted-tgz.zar downloaded.tgz
zarc create --block-size 4M --compression-level 7 backup.zar dataset/
zarc create --password secrets.zar private/
zarc create --user backup.zar docs/
```

`create` makes a new archive and refuses to replace an existing output file by
default. Pass `--overwrite` to atomically replace an existing archive after the
new archive has been created and validated.
Directories are scanned recursively. Paths inside the current working
directory are stored relative to it; an absolute input outside it is stored
using its basename (for example, `/var/log` is stored as `log`). Empty files
are supported.
ZIP, 7z, RAR, ISO, TAR, TAR.GZ, TGZ, and GZ files passed as input are imported by their
internal paths rather than stored as a single archive file. This also applies
to paths passed to `zarc add`. Encrypted ZIP/7z/RAR sources and ZIP/7z/RAR links are not
supported. Multi-volume RAR archives and RAR special entries are rejected.
ISO import uses ISO 9660 with Rock Ridge preferred over Joliet and the primary
namespace. ISO links and special entries and UDF-only images are not supported.
ISO imports are limited to 100,000 entries, 16 MiB of directory metadata, and
64 directory levels.
TAR symbolic links are preserved where the host supports them, and
hard links are imported as regular files. TAR paths with harmless `.` components
are normalized; parent traversal and special entries are rejected. Source
archive timestamps are not retained; timestamp options record the staged files' import-time
filesystem timestamps instead. 7z entries using COPY, LZMA/LZMA2, BZip2,
PPMD, DEFLATE, Brotli, LZ4, or Zstandard are supported.

Options:

- `--block-size SIZE`: `64K` through `32M`, or `auto`. The default is 8 MiB;
  `auto` selects a size based on available memory.
- `--compression-level LEVEL`: Zstandard levels `-7` through `22`; default `3`.
- `--password`: prompt without echo and encrypt headers and data blocks.
- `--overwrite`: replace the target archive if it already exists.
- `--metadata`: record supported platform-specific owner, group, and
  permission metadata.
- `--modification-time`, `--mt`: record each entry's modification time.
- `--creation-time`, `--ct`: record each entry's creation time; fail if it is
  unavailable for a stored entry.
- `--access-time`, `--at`: record each entry's access time; fail if it is
  unavailable for a stored entry. Filesystem policies and reads during scanning
  can affect access times. Timestamps are shown in `zarg`, but archive
  extraction does not restore them.
- `--special-files`: include named pipes and Unix device nodes; these are
  otherwise skipped with warnings.
- `--verbose`: print the warning when storing a symbolic link. By default,
  symlinks are stored without this warning.
- `--strict`: reject symlinks and unsupported special input types. Without
  strict mode, symlinks are recorded in the archive and require
  `extract --symlinks` to restore; unreadable files are skipped with a warning;
  named pipes and device nodes are skipped unless `--special-files` is set.
- `--include-file PATTERN` / `--exclude-file PATTERN` (aliases `--if` / `--ef`):
  filter files using fnmatch patterns. Repeat either option to provide multiple
  patterns.
- `--include-folder PATTERN` / `--exclude-folder PATTERN` (aliases `--id` /
  `--ed`): filter matching folders; matching a folder includes or excludes its
  entire subtree.
- `--filelist PATH`: read patterns from a text file, one per line. Lines
  beginning with `-` exclude the pattern; other lines include it. File-list
  patterns apply to files and folders.
- `-p, --progress`: show progress information.

Patterns match archive-relative paths with `/` separators. `*`, `?`, and
bracket expressions such as `[ab]` are supported; `*` can match across `/`.
When any include patterns are provided, unmatched files are omitted. Excludes
always override includes. Folder rules in file lists apply to matching folders
and their descendants.

The same file, folder, and file-list filters can be used with `zarc list` to
show only matching archive entries. For example, `zarc list backup.zar
--include-file '*.txt' --exclude-folder private` lists matching text files
while omitting the `private` subtree. Unlike extraction, listing does not add
unmatched parent directories to the filtered output.

The scanner deduplicates identical content within the created commit. Some
already-compressed formats and media types are stored without another
compression pass.

### Convert a folder of archives

```sh
zarc convert downloads/
zarc convert downloads/ --filter '*.zip'
zarc convert downloads/ --output-folder converted/ --filter '*.tar.gz'
zarc convert downloads/ --overwrite
```

`convert` scans only files directly inside the input folder (it does not scan
subfolders) and converts supported ZIP, 7z, RAR, ISO, TAR, TAR.GZ, TGZ, and GZ archives into
separate ZAR files. The fnmatch `--filter` is matched against each source
filename and defaults to `*`; unsupported files are ignored. The destination
folder defaults to the input folder. The source archive extension is replaced
with `.zar`, so `backup.tar.gz` becomes `backup.zar`.

Existing destination files are refused unless `--overwrite` is specified.
When multiple selected source names map to the same output name, that collision
is rejected even with `--overwrite`. The command continues after an individual
conversion failure, reports each failure, and returns exit code 1 if any
conversion fails or no supported archives match. `--block-size` and
`--compression-level` use the same options as `create`.

### Append, remove, and compact commits

```sh
zarc add backup.zar new-file.txt updated-docs/
zarc add --user backup.zar source/
zarc add --special-files backup.zar /dev/
zarc remove backup.zar obsolete.txt old-directory/
zarc join backup.zar --from 00000001 --to 00000004
zarc remove-commits backup.zar 00000002 00000004 --output reduced.zar
zarc meta backup.zar --set author=Jane --remove old-key
zarc meta backup.zar
```

`meta` lists, sets (`--set KEY=VALUE`) or removes (`--remove KEY`) custom
key/value metadata. Changes are stored in a new commit without file changes;
keys starting with `zar.` are reserved.

- `add` appends a commit. New or modified paths replace the corresponding
  entries in the latest TOC; untouched entries remain available.
- `remove` appends tombstones. Removing a directory removes its descendants
  from the effective TOC but does not reclaim historical data.
- `join` compacts an inclusive, contiguous range of commits into one commit at
  the same position, preserving the resulting TOC state and commits outside the
  range. Commit IDs are eight hexadecimal digits.
- `remove-commits` removes the specified commit IDs, preserving the other
  commits in order. At least one commit must remain; the first can be removed.
  Use `--output` to write a different archive, otherwise the input is replaced.
  Retained commits are replayed, so removing one can change subsequent state.
  An incompatible remaining history is rejected when all retained headers are
  readable. Encrypted headers are copied without passwords and their effects
  cannot be validated by this command.

`add` accepts `--block-size`, `--compression-level`, `--metadata`,
`--modification-time` (`--mt`), `--creation-time` (`--ct`), `--access-time` (`--at`),
`--special-files`, `--strict`, `--verbose`, the include/exclude filter options
above, and `--progress`. Unreadable files are skipped with a warning unless
`--strict` is set, in which case a permission error fails the operation. Named
pipes and Unix device nodes are skipped with warnings unless `--special-files`
is set. Device nodes are only created during extraction with `extract --devices`
and may require privileges. `join` accepts `--from`, `--to`, `--block-size`,
`--compression-level`, and `--progress`. The commands stage and validate the
replacement before publishing it. These state-changing operations currently
reject archives whose accumulated TOC depends on encrypted headers; encrypted
archive append/removal/compaction is not supported. `remove-commits` copies
retained commits without decrypting them and does not require passwords.

### Encrypt and decrypt commit ranges

```sh
zarc encrypt backup.zar --from 00000002 --to 00000003
zarc encrypt backup.zar --from 00000002 --to 00000003 --output encrypted-copy.zar
zarc decrypt encrypted-copy.zar --from 00000002 --to 00000003 --password --strict
```

`encrypt` and `decrypt` transform the inclusive range of commit IDs in physical
commit order. Without `--output`, the archive is transformed in place; when
specified, `--output FILE` (`-o FILE`) must identify a different path.
`encrypt` always prompts without echo for the new encryption password. It
accepts `--aead` and `--encryption-scope` to select the algorithm and encrypted
content.

For already-encrypted commits in the range, `--password` prompts for a default
current password and repeatable `--password-for COMMIT_ID` options prompt for
commit-specific current passwords. Without `--strict`, selected encrypted
commits that have no supplied current password are preserved unchanged;
`--strict` makes a missing password an error. A supplied but incorrect password
always fails the operation. For `decrypt`, `--password` and `--password-for`
provide the credentials used to decrypt commits.

### Inspect and verify

```sh
zarc list backup.zar
zarc list backup.zar --to 00000002
zarc info backup.zar
zarc info -v backup.zar
zarc info --json backup.zar
zarc check backup.zar
zarc check --password backup.zar
zarc check --recover backup.zar
```

- `list` displays the effective TOC without reading or decompressing block
  payloads. `--to ID` shows the state after that commit.
- `info` shows per-commit header content changes, commit IDs, stored sizes,
  creation date/time, optional username, logical uncompressed file sizes, and
  the ratio of commit size to uncompressed size. Header-encrypted commits show
  the uncompressed size as unavailable without credentials. `-v` adds
  per-file-group compression and deduplication statistics, plus counts for
  stored symlinks, pipes, and devices; use `list` to see their paths and device
  numbers. `-vv` additionally lists each block's compressed size, compression
  ratio, file count, and file groups. Both verbose levels read frame metadata
  without decompressing file contents. `--json` includes the header content
  change counts, formatted creation times and Unix-millisecond timestamps, and
  optional usernames in its machine-readable summary. `create --user` and
  `add --user` store the current OS username with the new commit; commit
  timestamps are always recorded. These statistics use frame metadata and do not
  verify or decompress data payloads.
- `check` validates framing, readable TOC updates, block references, and
  checksums; it decompresses unencrypted blocks and checks decoded file sizes.
  Pass `--password` to authenticate/decrypt encrypted content. Without the
  required password, encrypted portions remain unverified or unreadable.
- `--recover` permits `check` to scan past damaged framing to a later valid
  commit. It does not repair or change the file. The command reports recovery
  warnings and checks the recovered suffix independently.

### Extract files

```sh
zarc extract backup.zar --output ./restored/
zarc extract backup.zar -o ./restored/ --to 00000002
zarc extract backup.zar --dry-run
zarc extract backup.zar --output ./restored/ --strip-top-level
zarc extract backup.zar --collision rename --existing skip
zarc extract --password secrets.zar --output ./private/
```

`--output DIR` (`-o DIR`) selects the output directory; its default is the
current directory. Extraction plans and validates
the selected archive state, then streams decoded blocks to files and verifies
reconstructed file sizes.

- `--dry-run`: display planned directories and files without writing them.
- `--to ID`: extract the state as of the selected commit.
- `--collision POLICY`: handle archive paths that collide when case-folded:
  `rename` (default), `first`, `latest`, `error`, or interactive `ask`.
- `--existing POLICY`: handle files already at the destination: `error`
  (default), `skip`, `overwrite`, or interactive `ask`.
- `--metadata`: restore supported owner, group, and permissions.
- `--symlinks`: create symlinks whose targets remain within the extraction
  root. This is disabled by default.
- `--devices`: create archived Unix character and block devices; privileges
  may be required.
- `--password`: prompt for a password to unlock encrypted headers and blocks.
- `--include-file`, `--exclude-file`, `--include-folder`, `--exclude-folder`,
  and `--filelist`: select archive-relative paths using the filter rules
  described above.
- `-p, --progress`: show extraction progress.

`ask` requires an interactive terminal. Unix extraction uses directory-relative
no-follow operations to guard against symlink traversal and path-component
swaps. The non-Unix fallback uses standard path-based operations and does not
provide equivalent race resistance.

## Exit behavior

Successful commands return status `0`. The CLI uses distinct nonzero statuses
for usage errors, I/O errors, corrupt framing/container data, and warnings;
`check --recover` and warning-only unsupported inputs can complete with a
warning status. Treat any nonzero status as requiring attention and read the
reported diagnostic.

## Archive paths and encryption

Archive paths are canonical UTF-8 paths separated by `/`. Absolute paths,
drive prefixes, empty components, `.` and `..` components, NUL bytes, and
backslashes are not stored. During input scanning, entries whose archive path
contains a literal backslash are skipped with a `W_INVALID_ARCHIVE_PATH`
warning; `create --strict` fails instead. This preserves portable, unambiguous
archive paths.

Password-encrypted archives use Argon2id for password-based key wrapping and
AES-256-GCM for content encryption. Passwords are read without terminal echo.
Do not treat frame checksums as a security feature: checksums detect accidental
corruption but do not authenticate unencrypted data.
