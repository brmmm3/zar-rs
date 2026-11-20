# icons Archive Benchmark

Run date: 2026-11-20T08:42:20+01:00
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
| zarlib | No | hot | 23.5 MiB | 0.70-0.71 s (41.5-42.2 MiB/s) | 1.17-1.20 s (24.7-25.3 MiB/s) | 0.056-0.059 s | 0 B | 0 % / 0.03 GiB | 41 % / 0.11 GiB |
| zarlib | Yes | hot | 23.5 MiB | 0.73-0.77 s (38.2-40.2 MiB/s) | 1.19-1.28 s (23.0-24.8 MiB/s) | 0.127-0.131 s | 0 B | 0 % / 0.09 GiB | 38 % / 0.15 GiB |
| zip | No | hot | 31.4 MiB | 2.18-2.31 s (12.8-13.5 MiB/s) | 6.81-7.60 s (3.9-4.3 MiB/s) | 0.053-0.066 s | 0 B | 11 % / 0.03 GiB | 24 % / 0.02 GiB |
| zip | Yes | hot | 31.6 MiB | 2.45-2.63 s (11.2-12.0 MiB/s) | 7.53-8.05 s (3.7-3.9 MiB/s) | 0.054-0.059 s | 0 B | 11 % / 0.03 GiB | 28 % / 0.02 GiB |
| 7z | No | hot | 21.9 MiB | 1.28-1.31 s (22.4-23.1 MiB/s) | 5.77-5.80 s (5.1-5.1 MiB/s) | 0.161-0.164 s | 0 B | 10 % / 0.19 GiB | 22 % / 0.06 GiB |
| 7z | Yes | hot | 21.9 MiB | 1.28-1.40 s (21.1-23.0 MiB/s) | 5.83-5.89 s (5.0-5.1 MiB/s) | 0.172-0.178 s | 0 B | 10 % / 0.28 GiB | 21 % / 0.06 GiB |
| tar | No | hot | 35.4 MiB | 0.41-0.52 s (56.4-71.1 MiB/s) | 3.09-3.12 s (9.4-9.5 MiB/s) | 0.032-0.034 s | 0 B | N/A | 21 % / 0.01 GiB |
| tar | Yes | hot | 35.4 MiB | 0.58-0.60 s (49.5-50.9 MiB/s) | 3.31-3.35 s (8.8-8.9 MiB/s) | 0.227-0.229 s | 0 B | 0 % / 0.06 GiB | 21 % / 0.01 GiB |
| tar.gz | No | hot | 26.3 MiB | 0.99-1.02 s (28.9-29.7 MiB/s) | 3.32-3.36 s (8.8-8.9 MiB/s) | 0.091-0.098 s | 0 B | 0 % / 0.01 GiB | 27 % / 0.01 GiB |
| tar.gz | Yes | hot | 26.3 MiB | 1.07-1.10 s (26.9-27.5 MiB/s) | 3.41-3.58 s (8.2-8.6 MiB/s) | 0.233-0.236 s | 0 B | 18 % / 0.03 GiB | 40 % / 0.01 GiB |
| zarlib | No | cold | 23.5 MiB | 0.74-0.74 s (39.8-39.8 MiB/s) | 1.20-1.20 s (24.5-24.5 MiB/s) | 0.055-0.055 s | 0 B | 0 % / 0.03 GiB | 32 % / 0.11 GiB |
| zarlib | Yes | cold | 23.5 MiB | 0.76-0.76 s (38.6-38.6 MiB/s) | 1.26-1.26 s (23.4-23.4 MiB/s) | 0.136-0.136 s | 0 B | 0 % / 0.09 GiB | 38 % / 0.14 GiB |
| zip | No | cold | 31.4 MiB | 3.60-3.60 s (8.2-8.2 MiB/s) | 7.05-7.05 s (4.2-4.2 MiB/s) | 0.073-0.073 s | 0 B | 10 % / 0.03 GiB | 25 % / 0.02 GiB |
| zip | Yes | cold | 31.6 MiB | 4.21-4.21 s (7.0-7.0 MiB/s) | 7.42-7.42 s (4.0-4.0 MiB/s) | 0.070-0.070 s | 0 B | 18 % / 0.03 GiB | 23 % / 0.02 GiB |
| 7z | No | cold | 21.9 MiB | 1.95-1.95 s (15.1-15.1 MiB/s) | 5.62-5.62 s (5.2-5.2 MiB/s) | 0.157-0.157 s | 0 B | 6 % / 0.23 GiB | 21 % / 0.06 GiB |
| 7z | Yes | cold | 21.9 MiB | 1.89-1.89 s (15.6-15.6 MiB/s) | 5.74-5.74 s (5.1-5.1 MiB/s) | 0.170-0.170 s | 0 B | 8 % / 0.23 GiB | 21 % / 0.06 GiB |
| tar | No | cold | 35.4 MiB | 1.55-1.55 s (19.1-19.1 MiB/s) | 3.17-3.17 s (9.3-9.3 MiB/s) | 0.035-0.035 s | 0 B | 3 % / 0.01 GiB | 58 % / 0.01 GiB |
| tar | Yes | cold | 35.4 MiB | 1.67-1.67 s (17.7-17.7 MiB/s) | 3.35-3.35 s (8.8-8.8 MiB/s) | 0.244-0.244 s | 0 B | 4 % / 0.01 GiB | 33 % / 0.01 GiB |
| tar.gz | No | cold | 26.3 MiB | 2.11-2.11 s (14.0-14.0 MiB/s) | 3.15-3.15 s (9.4-9.4 MiB/s) | 0.096-0.096 s | 0 B | 3 % / 0.01 GiB | 25 % / 0.01 GiB |
| tar.gz | Yes | cold | 26.3 MiB | 2.20-2.20 s (13.4-13.4 MiB/s) | 3.33-3.33 s (8.9-8.9 MiB/s) | 0.255-0.255 s | 0 B | 3 % / 0.01 GiB | 28 % / 0.01 GiB |



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
