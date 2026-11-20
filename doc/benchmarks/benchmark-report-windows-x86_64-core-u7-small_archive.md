# small_archive Archive Benchmark

Run date: 2026-11-20T08:42:13+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-core-u7-small_archive.svg)

## Results

Source: [`small_archive.zar`](../../testdata/small_archive.zar)
Dataset: small Linux source tree from a ZAR archive
Uncompressed source size: 7.7 MiB
Regular files verified: 1,368
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 1.9 MiB | 0.19-0.19 s (39.9-41.1 MiB/s) | 0.56-0.58 s (13.3-13.6 MiB/s) | 0.058-0.061 s | 0 B | N/A | 0 % / 0.04 GiB |
| zarlib | Yes | hot | 1.9 MiB | 0.23-0.24 s (31.5-33.7 MiB/s) | 0.60-0.61 s (12.6-12.9 MiB/s) | 0.137-0.142 s | 0 B | N/A | 0 % / 0.09 GiB |
| zip | No | hot | 2.7 MiB | 0.49-0.53 s (14.6-15.9 MiB/s) | 1.30-1.34 s (5.8-5.9 MiB/s) | 0.050-0.053 s | 0 B | N/A | 14 % / 0.02 GiB |
| zip | Yes | hot | 2.8 MiB | 0.53-0.54 s (14.2-14.5 MiB/s) | 1.36-1.56 s (5.0-5.7 MiB/s) | 0.052-0.057 s | 0 B | 0 % / 0.02 GiB | 21 % / 0.02 GiB |
| 7z | No | hot | 1.5 MiB | 1.04-1.05 s (7.3-7.4 MiB/s) | 1.08-1.10 s (7.0-7.2 MiB/s) | 0.027-0.027 s | 0 B | 0 % / 0.07 GiB | 18 % / 0.01 GiB |
| 7z | Yes | hot | 1.5 MiB | 1.05-1.13 s (6.8-7.3 MiB/s) | 1.08-1.11 s (6.9-7.2 MiB/s) | 0.032-0.035 s | 0 B | 8 % / 0.09 GiB | 18 % / 0.02 GiB |
| tar | No | hot | 8.8 MiB | 0.09-0.10 s (80.2-86.2 MiB/s) | 0.61-0.64 s (12.1-12.6 MiB/s) | 0.019-0.026 s | 0 B | N/A | 0 % / 0.01 GiB |
| tar | Yes | hot | 8.8 MiB | 0.14-0.14 s (55.4-55.9 MiB/s) | 0.67-0.69 s (11.1-11.5 MiB/s) | 0.077-0.083 s | 0 B | N/A | 0 % / 0.01 GiB |
| tar.gz | No | hot | 2.0 MiB | 0.24-0.25 s (31.0-31.9 MiB/s) | 0.62-0.63 s (12.2-12.4 MiB/s) | 0.025-0.029 s | 0 B | N/A | 0 % / 0.01 GiB |
| tar.gz | Yes | hot | 2.0 MiB | 0.26-0.27 s (28.7-29.3 MiB/s) | 0.63-0.65 s (11.8-12.3 MiB/s) | 0.063-0.063 s | 0 B | N/A | 0 % / 0.01 GiB |
| zarlib | No | cold | 1.9 MiB | 0.20-0.20 s (39.3-39.3 MiB/s) | 0.58-0.58 s (13.4-13.4 MiB/s) | 0.054-0.054 s | 0 B | N/A | 92 % / 0.04 GiB |
| zarlib | Yes | cold | 1.9 MiB | 0.30-0.30 s (26.0-26.0 MiB/s) | 0.60-0.60 s (12.9-12.9 MiB/s) | 0.132-0.132 s | 0 B | N/A | 0 % / 0.09 GiB |
| zip | No | cold | 2.7 MiB | 0.92-0.92 s (8.4-8.4 MiB/s) | 1.46-1.46 s (5.3-5.3 MiB/s) | 0.061-0.061 s | 0 B | 0 % / 0.02 GiB | 21 % / 0.02 GiB |
| zip | Yes | cold | 2.8 MiB | 0.71-0.71 s (10.9-10.9 MiB/s) | 1.41-1.41 s (5.5-5.5 MiB/s) | 0.061-0.061 s | 0 B | 0 % / 0.02 GiB | 17 % / 0.02 GiB |
| 7z | No | cold | 1.5 MiB | 1.02-1.02 s (7.5-7.5 MiB/s) | 1.08-1.08 s (7.1-7.1 MiB/s) | 0.027-0.027 s | 0 B | 0 % / 0.07 GiB | 18 % / 0.02 GiB |
| 7z | Yes | cold | 1.5 MiB | 1.09-1.09 s (7.0-7.0 MiB/s) | 1.15-1.15 s (6.7-6.7 MiB/s) | 0.036-0.036 s | 0 B | 6 % / 0.04 GiB | 21 % / 0.02 GiB |
| tar | No | cold | 8.8 MiB | 0.25-0.25 s (30.6-30.6 MiB/s) | 0.63-0.63 s (12.3-12.3 MiB/s) | 0.023-0.023 s | 0 B | N/A | 0 % / 0.01 GiB |
| tar | Yes | cold | 8.8 MiB | 0.32-0.32 s (24.4-24.4 MiB/s) | 0.73-0.73 s (10.6-10.6 MiB/s) | 0.093-0.093 s | 0 B | N/A | 0 % / 0.01 GiB |
| tar.gz | No | cold | 2.0 MiB | 0.45-0.45 s (17.1-17.1 MiB/s) | 0.67-0.67 s (11.6-11.6 MiB/s) | 0.033-0.033 s | 0 B | N/A | 0 % / 0.01 GiB |
| tar.gz | Yes | cold | 2.0 MiB | 0.49-0.49 s (15.8-15.8 MiB/s) | 0.74-0.74 s (10.4-10.4 MiB/s) | 0.075-0.075 s | 0 B | N/A | 0 % / 0.01 GiB |



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
