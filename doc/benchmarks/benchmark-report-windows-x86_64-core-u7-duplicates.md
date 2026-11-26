# duplicates Archive Benchmark

Run date: 2026-11-26T15:59:34+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-core-u7-duplicates.svg)

## Results

Source: [`duplicates.zar`](../../testdata/duplicates.zar)
Dataset: duplicate icon tree from a ZAR archive
Uncompressed source size: 57.9 MiB
Regular files verified: 15,383
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.6 MiB | 1.20-1.34 s (43.3-48.1 MiB/s) | 2.13-2.27 s (25.5-27.2 MiB/s) | 0.058-0.069 s | 0 B | 7 % / 0.04 GiB | 34 % / 0.13 GiB |
| zarlib | Yes | hot | 23.6 MiB | 1.32-1.44 s (40.1-43.7 MiB/s) | 2.36-2.38 s (24.3-24.5 MiB/s) | 0.144-0.147 s | 0 B | 8 % / 0.10 GiB | 41 % / 0.16 GiB |
| zip | No | hot | 62.0 MiB | 4.45-4.71 s (12.3-13.0 MiB/s) | 13.00-13.78 s (4.2-4.5 MiB/s) | 0.057-0.071 s | 0 B | 12 % / 0.04 GiB | 20 % / 0.02 GiB |
| zip | Yes | hot | 62.4 MiB | 5.04-5.19 s (11.1-11.5 MiB/s) | 13.32-14.54 s (4.0-4.3 MiB/s) | 0.058-0.063 s | 0 B | 13 % / 0.04 GiB | 25 % / 0.02 GiB |
| 7z | No | hot | 22.0 MiB | 4.46-4.53 s (12.8-13.0 MiB/s) | 10.52-10.77 s (5.4-5.5 MiB/s) | 0.397-0.412 s | 0 B | 13 % / 0.27 GiB | 21 % / 0.09 GiB |
| 7z | Yes | hot | 22.0 MiB | 4.61-4.64 s (12.5-12.6 MiB/s) | 10.50-11.07 s (5.2-5.5 MiB/s) | 0.408-0.408 s | 0 B | 13 % / 0.27 GiB | 18 % / 0.09 GiB |
| tar | No | hot | 69.2 MiB | 0.83-0.84 s (69.0-69.3 MiB/s) | 4.90-4.98 s (11.6-11.8 MiB/s) | 0.045-0.051 s | 0 B | 20 % / 0.01 GiB | 19 % / 0.01 GiB |
| tar | Yes | hot | 69.2 MiB | 1.10-1.20 s (48.4-52.8 MiB/s) | 5.12-5.15 s (11.2-11.3 MiB/s) | 0.394-0.405 s | 0 B | 7 % / 0.06 GiB | 18 % / 0.01 GiB |
| tar.gz | No | hot | 51.7 MiB | 1.94-1.98 s (29.3-29.9 MiB/s) | 4.86-4.91 s (11.8-11.9 MiB/s) | 0.163-0.165 s | 0 B | 7 % / 0.01 GiB | 19 % / 0.01 GiB |
| tar.gz | Yes | hot | 51.7 MiB | 2.09-2.19 s (26.4-27.7 MiB/s) | 5.61-6.06 s (9.5-10.3 MiB/s) | 0.441-0.493 s | 0 B | 10 % / 0.03 GiB | 27 % / 0.01 GiB |
| zarlib | No | cold | 23.6 MiB | 1.95-1.95 s (29.7-29.7 MiB/s) | 2.31-2.31 s (25.0-25.0 MiB/s) | 0.076-0.076 s | 0 B | 17 % / 0.04 GiB | 49 % / 0.14 GiB |
| zarlib | Yes | cold | 23.6 MiB | 1.82-1.82 s (31.8-31.8 MiB/s) | 2.59-2.59 s (22.4-22.4 MiB/s) | 0.173-0.173 s | 0 B | 8 % / 0.10 GiB | 29 % / 0.13 GiB |
| zip | No | cold | 62.0 MiB | 9.72-9.72 s (6.0-6.0 MiB/s) | 12.06-12.06 s (4.8-4.8 MiB/s) | 0.074-0.074 s | 0 B | 11 % / 0.04 GiB | 14 % / 0.02 GiB |
| zip | Yes | cold | 62.4 MiB | 9.34-9.34 s (6.2-6.2 MiB/s) | 12.61-12.61 s (4.6-4.6 MiB/s) | 0.076-0.076 s | 0 B | 9 % / 0.03 GiB | 14 % / 0.02 GiB |
| 7z | No | cold | 22.0 MiB | 6.86-6.86 s (8.4-8.4 MiB/s) | 11.42-11.42 s (5.1-5.1 MiB/s) | 0.410-0.410 s | 0 B | 12 % / 0.25 GiB | 26 % / 0.09 GiB |
| 7z | Yes | cold | 22.0 MiB | 8.51-8.51 s (6.8-6.8 MiB/s) | 11.26-11.26 s (5.1-5.1 MiB/s) | 0.453-0.453 s | 0 B | 21 % / 0.25 GiB | 24 % / 0.09 GiB |
| tar | No | cold | 69.2 MiB | 7.13-7.13 s (8.1-8.1 MiB/s) | 5.51-5.51 s (10.5-10.5 MiB/s) | 0.114-0.114 s | 0 B | 16 % / 0.01 GiB | 25 % / 0.01 GiB |
| tar | Yes | cold | 69.2 MiB | 6.01-6.01 s (9.6-9.6 MiB/s) | 5.54-5.54 s (10.5-10.5 MiB/s) | 0.445-0.445 s | 0 B | 13 % / 0.01 GiB | 24 % / 0.01 GiB |
| tar.gz | No | cold | 51.7 MiB | 7.63-7.63 s (7.6-7.6 MiB/s) | 5.87-5.87 s (9.9-9.9 MiB/s) | 0.173-0.173 s | 0 B | 17 % / 0.01 GiB | 23 % / 0.01 GiB |
| tar.gz | Yes | cold | 51.7 MiB | 7.52-7.52 s (7.7-7.7 MiB/s) | 6.10-6.10 s (9.5-9.5 MiB/s) | 0.463-0.463 s | 0 B | 15 % / 0.01 GiB | 27 % / 0.01 GiB |



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
