# small_archive Archive Benchmark

Run date: 2026-11-20T07:11:18+01:00
- Operating system: Linux-5.15.0-198-generic-x86_64-with-glibc2.35
- CPU: Intel(R) Core(TM) i7-10700 CPU @ 2.90GHz (x86_64, 16 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-core-i7-small_archive.svg)

## Results

Source: [`small_archive.zar`](../../testdata/small_archive.zar)
Dataset: small Linux source tree from a ZAR archive
Uncompressed source size: 7.7 MiB
Regular files verified: 1,368
Symbolic links excluded from path/size verification: 7

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 1.9 MiB | 0.07-0.08 s (95.0-110.0 MiB/s) | 0.07-0.07 s (107.0-112.8 MiB/s) | 0.021-0.022 s | 0 B | N/A | N/A |
| zarlib | Yes | hot | 1.9 MiB | 0.14-0.16 s (47.4-53.5 MiB/s) | 0.17-0.18 s (41.9-45.8 MiB/s) | 0.118-0.120 s | 0 B | N/A | N/A |
| zip | No | hot | 2.6 MiB | 0.19-0.20 s (37.6-40.1 MiB/s) | 0.09-0.10 s (79.1-88.8 MiB/s) | 0.002-0.002 s | 0 B | N/A | N/A |
| zip | Yes | hot | 2.7 MiB | 0.20-0.21 s (36.7-37.8 MiB/s) | 0.11-0.11 s (67.2-68.7 MiB/s) | 0.005-0.005 s | 0 B | N/A | N/A |
| 7z | No | hot | 1.5 MiB | 1.18-1.29 s (6.0-6.5 MiB/s) | 0.13-0.17 s (46.2-60.5 MiB/s) | 0.012-0.012 s | 0 B | 12 % / 0.08 GiB | N/A |
| 7z | Yes | hot | 1.5 MiB | 1.30-1.45 s (5.3-6.0 MiB/s) | 0.25-0.26 s (30.2-30.3 MiB/s) | 0.144-0.146 s | 0 B | 14 % / 0.07 GiB | N/A |
| tar | No | hot | 8.8 MiB | 0.02-0.03 s (306.9-454.2 MiB/s) | 0.03-0.03 s (257.5-258.1 MiB/s) | 0.004-0.004 s | 0 B | N/A | N/A |
| tar | Yes | hot | 8.8 MiB | 0.05-0.08 s (98.3-165.7 MiB/s) | 0.05-0.06 s (128.7-149.8 MiB/s) | 0.029-0.049 s | 0 B | N/A | N/A |
| tar.gz | No | hot | 2.0 MiB | 0.21-0.23 s (33.0-36.3 MiB/s) | 0.04-0.04 s (177.7-181.0 MiB/s) | 0.039-0.039 s | 0 B | N/A | N/A |
| tar.gz | Yes | hot | 2.0 MiB | 0.22-0.23 s (34.2-34.9 MiB/s) | 0.05-0.05 s (147.4-149.3 MiB/s) | 0.048-0.049 s | 0 B | N/A | N/A |
| zarlib | No | cold | 1.9 MiB | 0.09-0.09 s (85.1-85.1 MiB/s) | 0.09-0.09 s (86.3-86.3 MiB/s) | 0.031-0.031 s | 0 B | N/A | N/A |
| zarlib | Yes | cold | 1.9 MiB | 0.18-0.18 s (43.6-43.6 MiB/s) | 0.18-0.18 s (43.2-43.2 MiB/s) | 0.133-0.133 s | 0 B | N/A | N/A |
| zip | No | cold | 2.6 MiB | 0.29-0.29 s (26.4-26.4 MiB/s) | 0.10-0.10 s (73.5-73.5 MiB/s) | 0.006-0.006 s | 0 B | N/A | N/A |
| zip | Yes | cold | 2.7 MiB | 0.32-0.32 s (24.3-24.3 MiB/s) | 0.12-0.12 s (65.8-65.8 MiB/s) | 0.009-0.009 s | 0 B | N/A | N/A |
| 7z | No | cold | 1.5 MiB | 1.33-1.33 s (5.8-5.8 MiB/s) | 0.18-0.18 s (42.0-42.0 MiB/s) | 0.025-0.025 s | 0 B | 7 % / 0.07 GiB | N/A |
| 7z | Yes | cold | 1.5 MiB | 1.45-1.45 s (5.3-5.3 MiB/s) | 0.35-0.35 s (22.0-22.0 MiB/s) | 0.163-0.163 s | 0 B | 15 % / 0.06 GiB | N/A |
| tar | No | cold | 8.8 MiB | 0.13-0.13 s (57.5-57.5 MiB/s) | 0.05-0.05 s (143.6-143.6 MiB/s) | 0.016-0.016 s | 0 B | N/A | N/A |
| tar | Yes | cold | 8.8 MiB | 0.15-0.15 s (50.7-50.7 MiB/s) | 0.07-0.07 s (104.9-104.9 MiB/s) | 0.051-0.051 s | 0 B | N/A | N/A |
| tar.gz | No | cold | 2.0 MiB | 0.23-0.23 s (33.5-33.5 MiB/s) | 0.07-0.07 s (115.6-115.6 MiB/s) | 0.065-0.065 s | 0 B | N/A | N/A |
| tar.gz | Yes | cold | 2.0 MiB | 0.26-0.26 s (29.5-29.5 MiB/s) | 0.08-0.08 s (101.4-101.4 MiB/s) | 0.082-0.082 s | 0 B | N/A | N/A |



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
on POSIX, ZIP and tar restore links, and 7z may skip unsafe relative links
during extraction, as noted above. Windows datasets are extracted without links.

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
- `zip`: Info-ZIP recursive DEFLATE at level 6, storing symlinks as links on POSIX.
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
- `tar`: `tar (GNU tar) 1.34`
- `openssl`: `OpenSSL 3.0.2 15 Mar 2022 (Library: OpenSSL 3.0.2 15 Mar 2022)`
- `7z`: `7-Zip [64] 16.02 : Copyright (c) 1999-2016 Igor Pavlov : 2016-05-21`
- `gzip`: `gzip 1.10`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
