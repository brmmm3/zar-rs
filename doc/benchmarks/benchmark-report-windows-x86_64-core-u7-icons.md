# icons Archive Benchmark

Run date: 2026-11-26T15:50:18+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-core-u7-icons.svg)

## Results

Source: [`icons.zar`](../../testdata/icons.zar)
Dataset: icon collection from a ZAR archive
Uncompressed source size: 29.5 MiB
Regular files verified: 7,958
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.5 MiB | 0.66-0.71 s (41.8-44.7 MiB/s) | 1.11-1.30 s (22.6-26.6 MiB/s) | 0.056-0.060 s | 0 B | 0 % / 0.03 GiB | 23 % / 0.11 GiB |
| zarlib | Yes | hot | 23.5 MiB | 0.71-0.80 s (37.0-41.7 MiB/s) | 1.08-1.20 s (24.6-27.2 MiB/s) | 0.127-0.130 s | 0 B | 0 % / 0.09 GiB | 51 % / 0.15 GiB |
| zip | No | hot | 31.4 MiB | 2.28-2.43 s (12.1-12.9 MiB/s) | 5.63-6.08 s (4.9-5.2 MiB/s) | 0.055-0.056 s | 0 B | 8 % / 0.03 GiB | 16 % / 0.02 GiB |
| zip | Yes | hot | 31.6 MiB | 2.41-2.51 s (11.7-12.2 MiB/s) | 5.81-5.83 s (5.1-5.1 MiB/s) | 0.050-0.058 s | 0 B | 8 % / 0.03 GiB | 12 % / 0.02 GiB |
| 7z | No | hot | 21.9 MiB | 1.25-1.33 s (22.2-23.6 MiB/s) | 4.86-5.03 s (5.9-6.1 MiB/s) | 0.156-0.157 s | 0 B | 13 % / 0.19 GiB | 30 % / 0.06 GiB |
| 7z | Yes | hot | 21.9 MiB | 1.29-1.36 s (21.8-22.8 MiB/s) | 4.77-5.02 s (5.9-6.2 MiB/s) | 0.168-0.176 s | 0 B | 13 % / 0.27 GiB | 18 % / 0.06 GiB |
| tar | No | hot | 35.4 MiB | 0.45-0.48 s (61.8-65.7 MiB/s) | 2.38-2.41 s (12.2-12.4 MiB/s) | 0.031-0.032 s | 0 B | N/A | 13 % / 0.01 GiB |
| tar | Yes | hot | 35.4 MiB | 0.63-0.64 s (46.4-47.2 MiB/s) | 2.81-2.87 s (10.3-10.5 MiB/s) | 0.215-0.229 s | 0 B | 0 % / 0.03 GiB | 21 % / 0.01 GiB |
| tar.gz | No | hot | 26.3 MiB | 0.99-1.02 s (28.9-29.9 MiB/s) | 2.41-2.50 s (11.8-12.2 MiB/s) | 0.090-0.093 s | 0 B | 0 % / 0.01 GiB | 14 % / 0.01 GiB |
| tar.gz | Yes | hot | 26.3 MiB | 1.11-1.12 s (26.4-26.5 MiB/s) | 2.54-2.65 s (11.1-11.6 MiB/s) | 0.236-0.278 s | 0 B | 31 % / 0.02 GiB | 31 % / 0.01 GiB |
| zarlib | No | cold | 23.5 MiB | 0.75-0.75 s (39.4-39.4 MiB/s) | 1.26-1.26 s (23.4-23.4 MiB/s) | 0.059-0.059 s | 0 B | 0 % / 0.03 GiB | 24 % / 0.11 GiB |
| zarlib | Yes | cold | 23.5 MiB | 0.81-0.81 s (36.6-36.6 MiB/s) | 1.40-1.40 s (21.1-21.1 MiB/s) | 0.127-0.127 s | 0 B | 0 % / 0.09 GiB | 24 % / 0.15 GiB |
| zip | No | cold | 31.4 MiB | 3.65-3.65 s (8.1-8.1 MiB/s) | 5.90-5.90 s (5.0-5.0 MiB/s) | 0.076-0.076 s | 0 B | 8 % / 0.03 GiB | 17 % / 0.02 GiB |
| zip | Yes | cold | 31.6 MiB | 4.27-4.27 s (6.9-6.9 MiB/s) | 5.83-5.83 s (5.1-5.1 MiB/s) | 0.070-0.070 s | 0 B | 8 % / 0.03 GiB | 13 % / 0.02 GiB |
| 7z | No | cold | 21.9 MiB | 2.04-2.04 s (14.5-14.5 MiB/s) | 4.87-4.87 s (6.1-6.1 MiB/s) | 0.163-0.163 s | 0 B | 5 % / 0.22 GiB | 17 % / 0.06 GiB |
| 7z | Yes | cold | 21.9 MiB | 1.92-1.92 s (15.4-15.4 MiB/s) | 4.85-4.85 s (6.1-6.1 MiB/s) | 0.178-0.178 s | 0 B | 8 % / 0.23 GiB | 16 % / 0.06 GiB |
| tar | No | cold | 35.4 MiB | 1.58-1.58 s (18.7-18.7 MiB/s) | 2.33-2.33 s (12.7-12.7 MiB/s) | 0.037-0.037 s | 0 B | 2 % / 0.01 GiB | 15 % / 0.01 GiB |
| tar | Yes | cold | 35.4 MiB | 1.80-1.80 s (16.4-16.4 MiB/s) | 2.63-2.63 s (11.2-11.2 MiB/s) | 0.234-0.234 s | 0 B | 4 % / 0.01 GiB | 16 % / 0.01 GiB |
| tar.gz | No | cold | 26.3 MiB | 2.29-2.29 s (12.9-12.9 MiB/s) | 2.55-2.55 s (11.6-11.6 MiB/s) | 0.099-0.099 s | 0 B | 7 % / 0.01 GiB | 18 % / 0.01 GiB |
| tar.gz | Yes | cold | 26.3 MiB | 2.61-2.61 s (11.3-11.3 MiB/s) | 2.65-2.65 s (11.1-11.1 MiB/s) | 0.248-0.248 s | 0 B | 6 % / 0.01 GiB | 17 % / 0.01 GiB |



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

- `zarlib`: `0.17.0`
- `cargo`: `cargo 1.101.0-nightly (3c747ea3e 2026-11-03)`
- `tar`: `bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.13.1-motley liblzma/5.8.3 bz2lib/1.0.8 libzstd/1.5.7 cng/2.0 libb2/bundled`
- `openssl`: `OpenSSL 4.1.0 23 Oct 2026 (Library: OpenSSL 4.1.0 23 Oct 2026)`
- `7z`: `7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-09-03`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send`
- `zarc`: `zarc 0.17.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
