# linux-6.1.1 Precompiled Kernel Archive Benchmark

Run date: 2026-11-20T10:09:45+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-core-u7-linux-6.1.1.svg)

## Results

Source: [`linux-6.1.1.zar`](../../testdata/linux-6.1.1.zar)
Dataset: precompiled Linux kernel tree from a ZAR archive
Uncompressed source size: 1445.3 MiB
Regular files verified: 88,384
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 213.5 MiB | 8.56-8.87 s (162.9-168.8 MiB/s) | 14.70-14.87 s (97.2-98.3 MiB/s) | 0.287-0.295 s | 0 B | 26 % / 0.22 GiB | 65 % / 0.57 GiB |
| zarlib | Yes | hot | 213.3 MiB | 8.27-8.84 s (163.4-174.8 MiB/s) | 15.31-15.76 s (91.7-94.4 MiB/s) | 0.398-0.404 s | 0 B | 24 % / 0.27 GiB | 59 % / 0.77 GiB |
| zip | No | hot | 310.5 MiB | 47.29-51.62 s (28.0-30.6 MiB/s) | 108.21-113.75 s (12.7-13.4 MiB/s) | 0.107-0.113 s | 0 B | 15 % / 0.10 GiB | 27 % / 0.02 GiB |
| zip | Yes | hot | 312.9 MiB | 51.11-51.91 s (27.8-28.3 MiB/s) | 109.01-115.85 s (12.5-13.3 MiB/s) | 0.107-0.111 s | 0 B | 15 % / 0.10 GiB | 27 % / 0.02 GiB |
| 7z | No | hot | 143.8 MiB | 59.94-63.65 s (22.7-24.1 MiB/s) | 93.10-100.35 s (14.4-15.5 MiB/s) | 1.071-1.109 s | 0 B | 60 % / 3.76 GiB | 36 % / 1.48 GiB |
| 7z | Yes | hot | 143.8 MiB | 61.27-62.72 s (23.0-23.6 MiB/s) | 89.95-92.80 s (15.6-16.1 MiB/s) | 1.088-1.103 s | 0 B | 62 % / 3.82 GiB | 31 % / 1.47 GiB |
| tar | No | hot | 1512.3 MiB | 7.85-17.29 s (83.6-184.1 MiB/s) | 56.81-58.33 s (24.8-25.4 MiB/s) | 0.361-0.427 s | 0 B | 23 % / 0.01 GiB | 45 % / 0.01 GiB |
| tar | Yes | hot | 1512.3 MiB | 14.32-15.21 s (95.1-101.0 MiB/s) | 60.18-61.93 s (23.3-24.0 MiB/s) | 9.539-10.707 s | 0 B | 23 % / 0.93 GiB | 32 % / 0.36 GiB |
| tar.gz | No | hot | 255.5 MiB | 30.93-33.70 s (42.9-46.7 MiB/s) | 53.14-57.40 s (25.2-27.2 MiB/s) | 1.720-1.749 s | 0 B | 26 % / 0.01 GiB | 39 % / 0.01 GiB |
| tar.gz | Yes | hot | 255.5 MiB | 30.47-30.65 s (47.2-47.4 MiB/s) | 44.33-46.05 s (31.4-32.6 MiB/s) | 3.215-3.288 s | 0 B | 18 % / 0.03 GiB | 40 % / 0.02 GiB |
| zarlib | No | cold | 213.4 MiB | 12.92-12.92 s (111.9-111.9 MiB/s) | 15.55-15.55 s (92.9-92.9 MiB/s) | 0.373-0.373 s | 0 B | 33 % / 0.17 GiB | 60 % / 0.53 GiB |
| zarlib | Yes | cold | 213.5 MiB | 13.45-13.45 s (107.5-107.5 MiB/s) | 15.94-15.94 s (90.7-90.7 MiB/s) | 0.475-0.475 s | 0 B | 39 % / 0.24 GiB | 71 % / 0.73 GiB |
| zip | No | cold | 310.5 MiB | 89.47-89.47 s (16.2-16.2 MiB/s) | 115.86-115.86 s (12.5-12.5 MiB/s) | 0.143-0.143 s | 0 B | 17 % / 0.10 GiB | 29 % / 0.02 GiB |
| zip | Yes | cold | 312.9 MiB | 91.63-91.63 s (15.8-15.8 MiB/s) | 117.92-117.92 s (12.3-12.3 MiB/s) | 0.234-0.234 s | 0 B | 17 % / 0.10 GiB | 27 % / 0.02 GiB |
| 7z | No | cold | 143.8 MiB | 86.91-86.91 s (16.6-16.6 MiB/s) | 87.00-87.00 s (16.6-16.6 MiB/s) | 1.094-1.094 s | 0 B | 51 % / 2.65 GiB | 32 % / 1.47 GiB |
| 7z | Yes | cold | 143.8 MiB | 88.26-88.26 s (16.4-16.4 MiB/s) | 90.95-90.95 s (15.9-15.9 MiB/s) | 1.096-1.096 s | 0 B | 43 % / 2.62 GiB | 31 % / 1.49 GiB |
| tar | No | cold | 1512.3 MiB | 42.95-42.95 s (33.6-33.6 MiB/s) | 55.00-55.00 s (26.3-26.3 MiB/s) | 2.049-2.049 s | 0 B | 21 % / 0.01 GiB | 40 % / 0.01 GiB |
| tar | Yes | cold | 1512.3 MiB | 45.97-45.97 s (31.4-31.4 MiB/s) | 58.11-58.11 s (24.9-24.9 MiB/s) | 8.921-8.921 s | 0 B | 17 % / 0.22 GiB | 29 % / 0.35 GiB |
| tar.gz | No | cold | 255.5 MiB | 60.44-60.44 s (23.9-23.9 MiB/s) | 51.27-51.27 s (28.2-28.2 MiB/s) | 1.625-1.625 s | 0 B | 16 % / 0.01 GiB | 30 % / 0.01 GiB |
| tar.gz | Yes | cold | 255.5 MiB | 66.39-66.39 s (21.8-21.8 MiB/s) | 54.89-54.89 s (26.3-26.3 MiB/s) | 2.939-2.939 s | 0 B | 23 % / 0.01 GiB | 37 % / 0.02 GiB |



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
- On this platform `zip`/`unzip` receive the password with `-P` instead of an interactive prompt.
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
- `cargo`: `cargo 1.101.0-nightly (3c747ea3e 2026-11-03)`
- `tar`: `bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.13.1-motley liblzma/5.8.3 bz2lib/1.0.8 libzstd/1.5.7 cng/2.0 libb2/bundled`
- `openssl`: `OpenSSL 4.1.0 23 Oct 2026 (Library: OpenSSL 4.1.0 23 Oct 2026)`
- `7z`: `7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-09-03`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
