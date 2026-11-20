# linux-6.1.1 Precompiled Kernel Archive Benchmark

Run date: 2026-11-20T20:20:52+01:00
- Operating system: Linux-7.2.15-070215-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-ryzen5-linux-6.1.1.svg)

## Results

Source: [`linux-6.1.1.zar`](../../testdata/linux-6.1.1.zar)
Dataset: precompiled Linux kernel tree from a ZAR archive
Uncompressed source size: 1445.3 MiB
Regular files verified: 88,384
Symbolic links excluded from path/size verification: 56

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 212.8 MiB | 5.81-6.12 s (236.1-248.8 MiB/s) | 5.82-5.88 s (245.8-248.1 MiB/s) | 0.417-0.522 s | 0 B | 81 % / 0.52 GiB | 81 % / 0.45 GiB |
| zarlib | Yes | hot | 212.7 MiB | 5.27-5.61 s (257.8-274.0 MiB/s) | 7.08-7.44 s (194.3-204.0 MiB/s) | 0.543-0.700 s | 0 B | 79 % / 0.57 GiB | 72 % / 0.55 GiB |
| zip | No | hot | 304.0 MiB | 44.18-46.93 s (30.8-32.7 MiB/s) | 21.62-24.02 s (60.2-66.8 MiB/s) | 0.025-0.040 s | 0 B | 57 % / 0.05 GiB | 86 % / 0.00 GiB |
| zip | Yes | hot | 306.3 MiB | 40.28-43.51 s (33.2-35.9 MiB/s) | 22.03-23.61 s (61.2-65.6 MiB/s) | 0.055-0.067 s | 0 B | 49 % / 0.05 GiB | 78 % / 0.00 GiB |
| 7z | No | hot | 144.0 MiB | 151.95-169.96 s (8.5-9.5 MiB/s) | 10.94-17.15 s (84.3-132.1 MiB/s) | 3.152-3.928 s | 0 B | 77 % / 1.75 GiB | 83 % / 0.81 GiB |
| 7z | Yes | hot | 144.0 MiB | 161.82-164.17 s (8.8-8.9 MiB/s) | 11.64-14.15 s (102.1-124.2 MiB/s) | 2.954-3.574 s | 0 B | 79 % / 1.76 GiB | 86 % / 0.98 GiB |
| tar | No | hot | 1512.4 MiB | 5.86-10.10 s (143.1-246.8 MiB/s) | 16.74-22.06 s (65.5-86.3 MiB/s) | 0.220-2.094 s | 0 B | 37 % / 0.00 GiB | 38 % / 0.00 GiB |
| tar | Yes | hot | 1512.4 MiB | 26.96-30.37 s (47.6-53.6 MiB/s) | 80.12-84.26 s (17.2-18.0 MiB/s) | 26.177-26.637 s | 0 B | 34 % / 1.37 GiB | 25 % / 0.68 GiB |
| tar.gz | No | hot | 262.1 MiB | 34.46-35.22 s (41.0-41.9 MiB/s) | 6.31-8.87 s (162.9-229.2 MiB/s) | 3.679-3.882 s | 0 B | 40 % / 0.01 GiB | 71 % / 0.01 GiB |
| tar.gz | Yes | hot | 262.1 MiB | 36.98-38.95 s (37.1-39.1 MiB/s) | 8.98-12.18 s (118.7-160.9 MiB/s) | 5.447-8.983 s | 0 B | 47 % / 0.03 GiB | 45 % / 0.04 GiB |
| zarlib | No | cold | 213.1 MiB | 7.59-7.59 s (190.5-190.5 MiB/s) | 7.33-7.33 s (197.3-197.3 MiB/s) | 0.686-0.686 s | 0 B | 86 % / 0.44 GiB | 73 % / 0.48 GiB |
| zarlib | Yes | cold | 213.3 MiB | 8.75-8.75 s (165.1-165.1 MiB/s) | 13.36-13.36 s (108.2-108.2 MiB/s) | 0.954-0.954 s | 0 B | 92 % / 0.46 GiB | 77 % / 0.57 GiB |
| zip | No | cold | 304.0 MiB | 69.02-69.02 s (20.9-20.9 MiB/s) | 37.93-37.93 s (38.1-38.1 MiB/s) | 0.069-0.069 s | 0 B | 63 % / 0.05 GiB | 93 % / 0.00 GiB |
| zip | Yes | cold | 306.3 MiB | 76.80-76.80 s (18.8-18.8 MiB/s) | 37.86-37.86 s (38.2-38.2 MiB/s) | 0.138-0.138 s | 0 B | 66 % / 0.05 GiB | 84 % / 0.00 GiB |
| 7z | No | cold | 144.0 MiB | 178.92-178.92 s (8.1-8.1 MiB/s) | 12.40-12.40 s (116.6-116.6 MiB/s) | 4.110-4.110 s | 0 B | 81 % / 1.72 GiB | 83 % / 0.98 GiB |
| 7z | Yes | cold | 144.0 MiB | 211.26-211.26 s (6.8-6.8 MiB/s) | 16.26-16.26 s (88.9-88.9 MiB/s) | 4.151-4.151 s | 0 B | 89 % / 1.75 GiB | 96 % / 1.00 GiB |
| tar | No | cold | 1512.4 MiB | 25.97-25.97 s (55.6-55.6 MiB/s) | 23.14-23.14 s (62.5-62.5 MiB/s) | 1.205-1.205 s | 0 B | 73 % / 0.00 GiB | 50 % / 0.00 GiB |
| tar | Yes | cold | 1512.4 MiB | 67.87-67.87 s (21.3-21.3 MiB/s) | 48.88-48.88 s (29.6-29.6 MiB/s) | 12.249-12.249 s | 0 B | 47 % / 0.49 GiB | 42 % / 0.45 GiB |
| tar.gz | No | cold | 262.1 MiB | 42.76-42.76 s (33.8-33.8 MiB/s) | 13.52-13.52 s (106.9-106.9 MiB/s) | 4.346-4.346 s | 0 B | 48 % / 0.01 GiB | 36 % / 0.01 GiB |
| tar.gz | Yes | cold | 262.1 MiB | 51.80-51.80 s (27.9-27.9 MiB/s) | 18.11-18.11 s (79.8-79.8 MiB/s) | 12.057-12.057 s | 0 B | 53 % / 0.02 GiB | 44 % / 0.03 GiB |



"Extract 3 files" is the time to extract only `Makefile`, `COPYING`, `kernel/sched/core.c`
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
