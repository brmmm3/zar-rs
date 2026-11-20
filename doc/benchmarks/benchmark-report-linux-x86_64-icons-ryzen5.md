# icons Archive Benchmark

Run date: 2026-11-20T06:00:12+01:00
- Operating system: Linux-7.2.15-070215-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-icons-ryzen5.svg)

## Results

Source: [`icons.zar`](../../testdata/icons.zar)
Dataset: icon collection from a ZAR archive
Uncompressed source size: 29.5 MiB
Regular files verified: 7,958
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.5 MiB | 0.18-0.21 s (141.8-167.7 MiB/s) | 0.38-0.39 s (74.7-77.7 MiB/s) | 0.059-0.079 s | 0 B | N/A | N/A |
| zarlib | Yes | hot | 23.5 MiB | 0.35-0.38 s (76.7-84.6 MiB/s) | 0.58-0.58 s (50.6-51.3 MiB/s) | 0.233-0.287 s | 0 B | N/A | 29 % / 0.16 GiB |
| zip | No | hot | 30.9 MiB | 1.39-1.42 s (20.8-21.2 MiB/s) | 0.85-1.11 s (26.7-34.7 MiB/s) | 0.006-0.006 s | 0 B | 56 % / 0.01 GiB | 88 % / 0.00 GiB |
| zip | Yes | hot | 31.1 MiB | 1.68-1.74 s (16.9-17.6 MiB/s) | 1.21-1.31 s (22.5-24.3 MiB/s) | 0.015-0.016 s | 0 B | 57 % / 0.01 GiB | 82 % / 0.00 GiB |
| 7z | No | hot | 21.9 MiB | 3.32-3.36 s (8.8-8.9 MiB/s) | 1.29-1.34 s (22.0-22.8 MiB/s) | 0.225-0.232 s | 0 B | 50 % / 0.17 GiB | 58 % / 0.06 GiB |
| 7z | Yes | hot | 21.9 MiB | 2.98-3.67 s (8.0-9.9 MiB/s) | 1.30-1.31 s (22.5-22.8 MiB/s) | 0.282-0.322 s | 0 B | 58 % / 0.19 GiB | 54 % / 0.06 GiB |
| tar | No | hot | 35.4 MiB | 0.14-0.16 s (184.1-207.2 MiB/s) | 0.31-0.32 s (92.4-94.7 MiB/s) | 0.017-0.020 s | 0 B | N/A | N/A |
| tar | Yes | hot | 35.4 MiB | 0.35-0.45 s (65.0-83.3 MiB/s) | 0.50-0.59 s (50.1-59.0 MiB/s) | 0.262-0.321 s | 0 B | N/A | 54 % / 0.00 GiB |
| tar.gz | No | hot | 28.1 MiB | 0.97-0.98 s (30.2-30.6 MiB/s) | 0.35-0.39 s (75.4-83.5 MiB/s) | 0.133-0.134 s | 0 B | 21 % / 0.01 GiB | N/A |
| tar.gz | Yes | hot | 28.1 MiB | 1.17-1.24 s (23.9-25.2 MiB/s) | 0.50-0.61 s (48.6-59.3 MiB/s) | 0.375-0.396 s | 0 B | 30 % / 0.02 GiB | 54 % / 0.01 GiB |
| zarlib | No | cold | 23.5 MiB | 0.28-0.28 s (104.8-104.8 MiB/s) | 0.45-0.45 s (66.0-66.0 MiB/s) | 0.068-0.068 s | 0 B | N/A | N/A |
| zarlib | Yes | cold | 23.5 MiB | 0.47-0.47 s (62.4-62.4 MiB/s) | 0.62-0.62 s (47.7-47.7 MiB/s) | 0.270-0.270 s | 0 B | N/A | 50 % / 0.16 GiB |
| zip | No | cold | 30.9 MiB | 2.48-2.48 s (11.9-11.9 MiB/s) | 0.99-0.99 s (29.9-29.9 MiB/s) | 0.014-0.014 s | 0 B | 60 % / 0.01 GiB | 76 % / 0.00 GiB |
| zip | Yes | cold | 31.1 MiB | 2.54-2.54 s (11.6-11.6 MiB/s) | 1.44-1.44 s (20.5-20.5 MiB/s) | 0.020-0.020 s | 0 B | 59 % / 0.01 GiB | 80 % / 0.00 GiB |
| 7z | No | cold | 21.9 MiB | 3.69-3.69 s (8.0-8.0 MiB/s) | 1.32-1.32 s (22.4-22.4 MiB/s) | 0.360-0.360 s | 0 B | 59 % / 0.18 GiB | 51 % / 0.06 GiB |
| 7z | Yes | cold | 21.9 MiB | 3.26-3.26 s (9.0-9.0 MiB/s) | 1.32-1.32 s (22.4-22.4 MiB/s) | 0.266-0.266 s | 0 B | 47 % / 0.16 GiB | 53 % / 0.06 GiB |
| tar | No | cold | 35.4 MiB | 1.17-1.17 s (25.3-25.3 MiB/s) | 0.38-0.38 s (77.0-77.0 MiB/s) | 0.044-0.044 s | 0 B | 42 % / 0.00 GiB | N/A |
| tar | Yes | cold | 35.4 MiB | 1.45-1.45 s (20.3-20.3 MiB/s) | 0.69-0.69 s (43.0-43.0 MiB/s) | 0.288-0.288 s | 0 B | 34 % / 0.00 GiB | 41 % / 0.00 GiB |
| tar.gz | No | cold | 28.1 MiB | 1.40-1.40 s (21.1-21.1 MiB/s) | 0.41-0.41 s (72.5-72.5 MiB/s) | 0.154-0.154 s | 0 B | 35 % / 0.01 GiB | N/A |
| tar.gz | Yes | cold | 28.1 MiB | 1.96-1.96 s (15.1-15.1 MiB/s) | 0.67-0.67 s (44.1-44.1 MiB/s) | 0.369-0.369 s | 0 B | 56 % / 0.01 GiB | 49 % / 0.01 GiB |



"Extract 3 files" is the time to extract only `128x128/actions/address-book-new.png`, `128x128/places/folder.png`, `16x16/actions/address-book-new.png`
from the archive (for encrypted tar/tar.gz this includes decrypting the archive). "Temp space"
is the peak disk usage of the extraction output and the runner's temporary
directory during that extraction minus the size of the extracted files, sampled
every 5 ms (the maximum over all runs). Temporary files outside these
directories (e.g. in the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/extracted/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 4 extraction run(s), `zarlib` renamed 0 path(s) total (0 per run) due to case-folded collisions; generated suffixes are normalized only for comparison. The results
directory was emptied again after each check. Symlink entries are excluded
from verification. `zarlib` stores links but does not restore them by default;
ZIP and tar restore links, and 7z may skip unsafe relative links during
extraction, as noted above.

CPU utilisation and RAM consumption were sampled at 1-second intervals during
each compression and extraction operation using `psutil`. CPU is system-wide;
RAM is the summed resident set size (RSS) of the archiver process and its
children, excluding the runner and unrelated processes. Shared memory may be
counted more than once. Samples are taken only while a benchmark process is
running; short operations may have no samples and are shown as N/A. The table
shows the average CPU % and RAM GiB across available samples for each phase.
Runtime values show
the minimum and maximum measured time for each phase. The charts plot full
extraction times and the times to extract the three selected files separately.
The time-series charts plot CPU and RAM on separate axes for compression and
extraction, with one line per archive format (solid = plain, dashed = encrypted).
The CPU model and operating system are collected from the host. Storage details
describe the volumes holding the source/archive files and extraction output.
The operating system may not expose physical media behind virtual or layered
storage; such cases are identified as unknown or non-rotational/possibly virtual.

## Method

- **Cache state**: `hot` runs read every file of the source tree (and, before
  extraction, the archive) once before the measured operation. `cold` runs drop
  the operating system file cache before every measured operation: `sync` plus
  writing `3` to `/proc/sys/vm/drop_caches` on Linux (root required), or
  `RAMMap.exe -Et` on Windows (elevation required). Cold runs are skipped with
  a warning when the privileges are missing.
- The installed 7-Zip does not support the zstd codec (only forks such as 7-Zip-zstd do); the `7z-zstd` benchmark is skipped.
- `zarlib`: `zarc` release build, default Zstandard compression level 3;
  encrypted runs use AES-256-GCM and the default header-and-data scope.
- `zip`: Info-ZIP recursive DEFLATE at level 6, storing symlinks as links.
  Encrypted runs use the implementation's standard ZipCrypto mode
  (legacy/weak encryption).
- `7z-zstd`: 7z format with the Zstandard codec at level 3 (only run when the
  installed 7-Zip supports it, e.g. 7-Zip-zstd); otherwise like `7z`.
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
- Compression and extraction are measured twice per hot-cache configuration
  and once per cold-cache configuration. The report shows the minimum and
  maximum runtime across those runs. If an archiver command fails in any
  repetition, its configuration is marked FAILED and its error is shown above;
  other runs continue. The runner waits 3 seconds between
  benchmark runs; this delay is not included in the measured operation times.
  Build time, source download/extraction, and post-extraction verification are
  also excluded from the measured operation times.

## Tool versions

- `zarlib`: `0.15.0`
- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
