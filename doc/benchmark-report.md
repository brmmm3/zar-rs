# Linux 4.0.1 Archive Benchmark

Run date: 2026-10-30T07:25:17+01:00

![Archive size, compression time, and extraction time charts](./benchmark-charts.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)  
Uncompressed source size: 525.1 MiB  
Regular files verified: 48,945  
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression | Extraction |
|---|---:|---:|---:|---:|---:|
| zarlib | No | hot | 152.0 MiB | 4.60 s (114.1 MiB/s) | 4.87 s (107.7 MiB/s) |
| zarlib | Yes | hot | 153.3 MiB | 6.04 s (87.0 MiB/s) | 5.89 s (89.2 MiB/s) |
| zip | No | hot | 150.3 MiB | 19.60 s (26.8 MiB/s) | 10.92 s (48.1 MiB/s) |
| zip | Yes | hot | 151.6 MiB | 21.91 s (24.0 MiB/s) | 12.78 s (41.1 MiB/s) |
| 7z | No | hot | 80.2 MiB | 93.36 s (5.6 MiB/s) | 5.68 s (92.4 MiB/s) |
| 7z | Yes | hot | 80.2 MiB | 94.67 s (5.5 MiB/s) | 7.53 s (69.8 MiB/s) |
| tar | No | hot | 562.7 MiB | 1.53 s (343.7 MiB/s) | 4.80 s (109.3 MiB/s) |
| tar | Yes | hot | 562.7 MiB | 6.11 s (85.9 MiB/s) | 10.07 s (52.1 MiB/s) |
| tar.gz | No | hot | 122.2 MiB | 17.48 s (30.0 MiB/s) | 5.91 s (88.9 MiB/s) |
| tar.gz | Yes | hot | 122.2 MiB | 18.23 s (28.8 MiB/s) | 4.79 s (109.5 MiB/s) |

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 2 extraction run(s), `zarlib` renamed 24 path(s) total (12 per run) due to case-folded collisions; generated suffixes are normalized only for comparison. The results
directory was emptied again after each check. Symlink entries are excluded
because `zarlib` currently reports and skips unsupported input symlinks. ZIP
and tar preserve links; 7z preserves them in its archive but may skip unsafe
relative links during extraction, as noted above.

## Method

- **Cache state**: `hot` runs read every file of the source tree (and, before
  extraction, the archive) once before the measured operation. `cold` runs drop
  the operating system file cache before every measured operation: `sync` plus
  writing `3` to `/proc/sys/vm/drop_caches` on Linux (root required), or
  `RAMMap.exe -Et` on Windows (elevation required). Cold runs are skipped with
  a warning when the privileges are missing.

- `zarlib`: `zarc` release build, default Zstandard compression level 3;
  encrypted runs use AES-256-GCM and the default header-and-data scope.
- `zip`: Info-ZIP recursive DEFLATE at level 6, storing symlinks as links.
  Encrypted runs use the implementation's standard ZipCrypto mode
  (legacy/weak encryption).
- `7z`: 7z format with LZMA2 at level 5. Encrypted runs use AES-256 and
  encrypted headers; symlinks are stored as links. The 7-Zip CLI accepts
  passwords only as a command-line switch; this runner generates a temporary
  random password and never saves it. During extraction, 7-Zip rejects
  relative symlink targets containing parent components; the runner
  accepts only the expected warnings for those source links and still
  verifies every regular file.
- `tar`: GNU tar without compression (a pure archiving baseline). The
  encrypted run times tar plus OpenSSL CMS AES-256-GCM encryption and is
  decrypted before extraction, like `tar.gz`.
- `tar.gz`: GNU tar with gzip's default compression level. Since tar.gz has no
  built-in encryption, the encrypted run times tar/gzip plus OpenSSL CMS
  AES-256-GCM encryption. It is decrypted before extraction. A temporary
  self-signed RSA certificate is used to wrap the CMS content key; the
  temporary private key is deleted when the benchmark exits.
- Compression and extraction are each measured once per format/configuration.
  Build time, source download/extraction, and post-extraction verification are
  excluded from the measured operation times.

## Tool versions

- `cargo`: `cargo 1.101.0-nightly (cad07cf8e 2026-10-23)`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
