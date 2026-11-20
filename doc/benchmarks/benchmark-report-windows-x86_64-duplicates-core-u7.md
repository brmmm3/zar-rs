# duplicates Archive Benchmark

Run date: 2026-11-20T08:42:31+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-duplicates-core-u7.svg)

## Results

Source: [`duplicates.zar`](../../testdata/duplicates.zar)
Dataset: duplicate icon tree from a ZAR archive
Uncompressed source size: 57.9 MiB
Regular files verified: 15,383
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.6 MiB | 1.20-1.23 s (47.1-48.3 MiB/s) | 2.10-2.11 s (27.5-27.5 MiB/s) | 0.062-0.064 s | 0 B | 5 % / 0.04 GiB | 36 % / 0.12 GiB |
| zarlib | Yes | hot | 23.6 MiB | 1.26-1.32 s (43.7-45.9 MiB/s) | 2.27-2.38 s (24.3-25.5 MiB/s) | 0.142-0.146 s | 0 B | 6 % / 0.09 GiB | 47 % / 0.16 GiB |
| zip | No | hot | 62.0 MiB | 4.07-4.21 s (13.8-14.2 MiB/s) | 12.79-12.82 s (4.5-4.5 MiB/s) | 0.057-0.060 s | 0 B | 10 % / 0.03 GiB | 21 % / 0.02 GiB |
| zip | Yes | hot | 62.4 MiB | 4.52-4.78 s (12.1-12.8 MiB/s) | 13.08-13.25 s (4.4-4.4 MiB/s) | 0.060-0.064 s | 0 B | 11 % / 0.04 GiB | 21 % / 0.02 GiB |
| 7z | No | hot | 22.0 MiB | 4.94-4.97 s (11.6-11.7 MiB/s) | 12.27-13.47 s (4.3-4.7 MiB/s) | 0.417-0.425 s | 0 B | 17 % / 0.27 GiB | 26 % / 0.09 GiB |
| 7z | Yes | hot | 22.0 MiB | 4.84-5.32 s (10.9-12.0 MiB/s) | 12.55-13.46 s (4.3-4.6 MiB/s) | 0.431-0.437 s | 0 B | 14 % / 0.28 GiB | 25 % / 0.09 GiB |
| tar | No | hot | 69.2 MiB | 0.84-0.85 s (67.8-68.5 MiB/s) | 7.05-7.13 s (8.1-8.2 MiB/s) | 0.053-0.053 s | 0 B | 0 % / 0.01 GiB | 25 % / 0.01 GiB |
| tar | Yes | hot | 69.2 MiB | 1.06-1.09 s (53.3-54.7 MiB/s) | 6.19-6.78 s (8.5-9.3 MiB/s) | 0.375-0.401 s | 0 B | 8 % / 0.08 GiB | 31 % / 0.01 GiB |
| tar.gz | No | hot | 51.7 MiB | 1.94-1.95 s (29.7-29.8 MiB/s) | 6.76-6.84 s (8.5-8.6 MiB/s) | 0.168-0.175 s | 0 B | 8 % / 0.01 GiB | 27 % / 0.01 GiB |
| tar.gz | Yes | hot | 51.7 MiB | 2.11-2.20 s (26.3-27.4 MiB/s) | 6.77-6.85 s (8.5-8.6 MiB/s) | 0.419-0.425 s | 0 B | 8 % / 0.03 GiB | 28 % / 0.01 GiB |
| zarlib | No | cold | 23.6 MiB | 1.58-1.58 s (36.7-36.7 MiB/s) | 2.12-2.12 s (27.3-27.3 MiB/s) | 0.077-0.077 s | 0 B | 11 % / 0.04 GiB | 35 % / 0.13 GiB |
| zarlib | Yes | cold | 23.6 MiB | 1.58-1.58 s (36.6-36.6 MiB/s) | 2.29-2.29 s (25.3-25.3 MiB/s) | 0.154-0.154 s | 0 B | 13 % / 0.11 GiB | 40 % / 0.16 GiB |
| zip | No | cold | 62.0 MiB | 8.45-8.45 s (6.8-6.8 MiB/s) | 12.63-12.63 s (4.6-4.6 MiB/s) | 0.072-0.072 s | 0 B | 8 % / 0.03 GiB | 22 % / 0.02 GiB |
| zip | Yes | cold | 62.4 MiB | 8.88-8.88 s (6.5-6.5 MiB/s) | 13.11-13.11 s (4.4-4.4 MiB/s) | 0.076-0.076 s | 0 B | 11 % / 0.03 GiB | 21 % / 0.02 GiB |
| 7z | No | cold | 22.0 MiB | 7.10-7.10 s (8.2-8.2 MiB/s) | 11.35-11.35 s (5.1-5.1 MiB/s) | 0.422-0.422 s | 0 B | 10 % / 0.26 GiB | 23 % / 0.09 GiB |
| 7z | Yes | cold | 22.0 MiB | 7.87-7.87 s (7.4-7.4 MiB/s) | 11.68-11.68 s (5.0-5.0 MiB/s) | 0.438-0.438 s | 0 B | 15 % / 0.26 GiB | 27 % / 0.09 GiB |
| tar | No | cold | 69.2 MiB | 5.07-5.07 s (11.4-11.4 MiB/s) | 7.03-7.03 s (8.2-8.2 MiB/s) | 0.058-0.058 s | 0 B | 10 % / 0.01 GiB | 30 % / 0.01 GiB |
| tar | Yes | cold | 69.2 MiB | 5.37-5.37 s (10.8-10.8 MiB/s) | 6.78-6.78 s (8.5-8.5 MiB/s) | 0.411-0.411 s | 0 B | 6 % / 0.01 GiB | 27 % / 0.01 GiB |
| tar.gz | No | cold | 51.7 MiB | 6.09-6.09 s (9.5-9.5 MiB/s) | 6.67-6.67 s (8.7-8.7 MiB/s) | 0.167-0.167 s | 0 B | 6 % / 0.01 GiB | 26 % / 0.01 GiB |
| tar.gz | Yes | cold | 51.7 MiB | 6.31-6.31 s (9.2-9.2 MiB/s) | 7.24-7.24 s (8.0-8.0 MiB/s) | 0.436-0.436 s | 0 B | 8 % / 0.02 GiB | 28 % / 0.01 GiB |



"Extract 3 files" is the time to extract only `duplicates/128x128/actions/address-book-new.png`, `duplicates/128x128/places/folder.png`, `icons/128x128/actions/address-book-new.png`
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
