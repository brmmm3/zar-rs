# small_archive Archive Benchmark

Run date: 2026-11-20T06:00:10+01:00
- Operating system: Linux-7.2.15-070215-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-small_archive-ryzen5.svg)

## Results

Source: [`small_archive.zar`](../../testdata/small_archive.zar)
Dataset: small Linux source tree from a ZAR archive
Uncompressed source size: 7.7 MiB
Regular files verified: 1,368
Symbolic links excluded from path/size verification: 7

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 1.9 MiB | 0.14-0.19 s (39.9-56.1 MiB/s) | 0.21-0.21 s (37.0-37.2 MiB/s) | 0.081-0.103 s | 0 B | N/A | N/A |
| zarlib | Yes | hot | 1.9 MiB | 0.30-0.35 s (21.8-25.8 MiB/s) | 0.39-0.42 s (18.4-19.9 MiB/s) | 0.294-0.313 s | 0 B | N/A | N/A |
| zip | No | hot | 2.6 MiB | 0.30-0.32 s (24.0-25.4 MiB/s) | 0.18-0.19 s (40.2-41.8 MiB/s) | 0.004-0.005 s | 0 B | N/A | N/A |
| zip | Yes | hot | 2.7 MiB | 0.33-0.35 s (22.0-23.3 MiB/s) | 0.22-0.22 s (35.3-35.4 MiB/s) | 0.026-0.030 s | 0 B | N/A | N/A |
| 7z | No | hot | 1.5 MiB | 2.00-2.02 s (3.8-3.9 MiB/s) | 0.20-0.21 s (36.8-37.9 MiB/s) | 0.017-0.019 s | 0 B | 28 % / 0.07 GiB | N/A |
| 7z | Yes | hot | 1.5 MiB | 2.08-2.48 s (3.1-3.7 MiB/s) | 0.22-0.30 s (26.1-35.1 MiB/s) | 0.038-0.056 s | 0 B | 60 % / 0.08 GiB | N/A |
| tar | No | hot | 8.8 MiB | 0.04-0.04 s (199.6-205.7 MiB/s) | 0.06-0.07 s (111.4-123.6 MiB/s) | 0.005-0.005 s | 0 B | N/A | N/A |
| tar | Yes | hot | 8.8 MiB | 0.09-0.10 s (73.7-86.6 MiB/s) | 0.11-0.33 s (23.5-69.6 MiB/s) | 0.070-0.217 s | 0 B | N/A | N/A |
| tar.gz | No | hot | 2.0 MiB | 0.26-0.27 s (28.8-29.1 MiB/s) | 0.08-0.09 s (86.4-95.8 MiB/s) | 0.033-0.038 s | 0 B | N/A | N/A |
| tar.gz | Yes | hot | 2.0 MiB | 0.28-0.28 s (27.2-27.4 MiB/s) | 0.09-0.10 s (79.7-82.3 MiB/s) | 0.065-0.077 s | 0 B | N/A | N/A |
| zarlib | No | cold | 1.9 MiB | 0.31-0.31 s (24.8-24.8 MiB/s) | 0.20-0.20 s (39.4-39.4 MiB/s) | 0.105-0.105 s | 0 B | N/A | N/A |
| zarlib | Yes | cold | 1.9 MiB | 0.36-0.36 s (21.4-21.4 MiB/s) | 0.41-0.41 s (18.6-18.6 MiB/s) | 0.380-0.380 s | 0 B | N/A | N/A |
| zip | No | cold | 2.6 MiB | 0.56-0.56 s (13.7-13.7 MiB/s) | 0.23-0.23 s (34.2-34.2 MiB/s) | 0.020-0.020 s | 0 B | 0 % / 0.00 GiB | N/A |
| zip | Yes | cold | 2.7 MiB | 0.52-0.52 s (14.9-14.9 MiB/s) | 0.28-0.28 s (28.0-28.0 MiB/s) | 0.017-0.017 s | 0 B | N/A | N/A |
| 7z | No | cold | 1.5 MiB | 2.22-2.22 s (3.5-3.5 MiB/s) | 0.22-0.22 s (35.4-35.4 MiB/s) | 0.036-0.036 s | 0 B | 57 % / 0.04 GiB | N/A |
| 7z | Yes | cold | 1.5 MiB | 2.02-2.02 s (3.8-3.8 MiB/s) | 0.25-0.25 s (31.0-31.0 MiB/s) | 0.069-0.069 s | 0 B | 50 % / 0.07 GiB | N/A |
| tar | No | cold | 8.8 MiB | 0.23-0.23 s (33.4-33.4 MiB/s) | 0.10-0.10 s (79.0-79.0 MiB/s) | 0.020-0.020 s | 0 B | N/A | N/A |
| tar | Yes | cold | 8.8 MiB | 0.49-0.49 s (15.8-15.8 MiB/s) | 0.69-0.69 s (11.1-11.1 MiB/s) | 0.222-0.222 s | 0 B | N/A | 44 % / 0.00 GiB |
| tar.gz | No | cold | 2.0 MiB | 0.40-0.40 s (19.1-19.1 MiB/s) | 0.11-0.11 s (69.1-69.1 MiB/s) | 0.046-0.046 s | 0 B | N/A | N/A |
| tar.gz | Yes | cold | 2.0 MiB | 0.38-0.38 s (20.5-20.5 MiB/s) | 0.14-0.14 s (53.3-53.3 MiB/s) | 0.095-0.095 s | 0 B | N/A | N/A |



"Extract 3 files" is the time to extract only `COPYING`, `Makefile`, `init/main.c`
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
