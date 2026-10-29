# Linux 4.0.1 Archive Benchmark

Run date: 2026-10-29T22:15:22+01:00

![Archive size, compression time, and extraction time charts](./benchmark-charts.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)  
Uncompressed source size: 525.1 MiB  
Regular files verified: 48,945  
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Archive size | Compression | Extraction |
|---|---:|---:|---:|---:|
| zarlib | No | 152.0 MiB | 5.27 s (99.7 MiB/s) | 5.31 s (98.9 MiB/s) |
| zarlib | Yes | 153.3 MiB | 7.25 s (72.5 MiB/s) | 7.91 s (66.4 MiB/s) |
| zip | No | 150.3 MiB | 20.33 s (25.8 MiB/s) | 11.08 s (47.4 MiB/s) |
| zip | Yes | 151.6 MiB | 26.28 s (20.0 MiB/s) | 16.74 s (31.4 MiB/s) |
| 7z | No | 80.2 MiB | 108.86 s (4.8 MiB/s) | 14.21 s (37.0 MiB/s) |
| 7z | Yes | 80.2 MiB | 109.12 s (4.8 MiB/s) | 7.07 s (74.2 MiB/s) |
| tar.gz | No | 122.2 MiB | 18.11 s (29.0 MiB/s) | 5.60 s (93.7 MiB/s) |
| tar.gz | Yes | 122.2 MiB | 22.33 s (23.5 MiB/s) | 10.90 s (48.2 MiB/s) |

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its two extraction runs,
`zarlib` renamed 24 paths total (12 per run) due to case-folded collisions;
generated suffixes are normalized only for comparison. The results directory
was emptied again after each check. Symlink entries are excluded because
`zarlib` currently reports and skips unsupported input symlinks. ZIP and tar
preserve links; 7z preserves them in its archive but may skip unsafe relative
links during extraction, as noted above.

## Method

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
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
