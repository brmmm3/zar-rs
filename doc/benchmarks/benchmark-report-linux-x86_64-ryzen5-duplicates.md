# duplicates Archive Benchmark

Run date: 2026-11-20T06:00:14+01:00
- Operating system: Linux-7.2.15-070215-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-ryzen5-duplicates.svg)

## Results

Source: [`duplicates.zar`](../../testdata/duplicates.zar)
Dataset: duplicate icon tree from a ZAR archive
Uncompressed source size: 57.9 MiB
Regular files verified: 15,383
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.6 MiB | 0.29-0.33 s (177.3-197.1 MiB/s) | 0.68-0.80 s (72.5-85.1 MiB/s) | 0.079-0.084 s | 0 B | N/A | 0 % / 0.15 GiB |
| zarlib | Yes | hot | 23.6 MiB | 0.45-0.55 s (106.1-129.6 MiB/s) | 0.87-0.98 s (59.1-66.7 MiB/s) | 0.288-0.348 s | 0 B | N/A | 58 % / 0.17 GiB |
| zip | No | hot | 60.9 MiB | 2.58-2.60 s (22.2-22.4 MiB/s) | 1.68-2.00 s (28.9-34.4 MiB/s) | 0.007-0.009 s | 0 B | 67 % / 0.01 GiB | 82 % / 0.00 GiB |
| zip | Yes | hot | 61.3 MiB | 2.81-2.98 s (19.4-20.6 MiB/s) | 2.46-2.49 s (23.2-23.6 MiB/s) | 0.024-0.027 s | 0 B | 55 % / 0.01 GiB | 71 % / 0.00 GiB |
| 7z | No | hot | 22.0 MiB | 8.54-8.94 s (6.5-6.8 MiB/s) | 1.95-1.97 s (29.4-29.7 MiB/s) | 0.595-0.603 s | 0 B | 50 % / 0.26 GiB | 55 % / 0.07 GiB |
| 7z | Yes | hot | 22.0 MiB | 8.36-8.72 s (6.6-6.9 MiB/s) | 1.95-1.96 s (29.5-29.7 MiB/s) | 0.621-0.637 s | 0 B | 48 % / 0.25 GiB | 52 % / 0.07 GiB |
| tar | No | hot | 69.2 MiB | 0.27-0.28 s (209.5-216.9 MiB/s) | 0.61-0.62 s (93.9-95.2 MiB/s) | 0.026-0.033 s | 0 B | N/A | 27 % / 0.00 GiB |
| tar | Yes | hot | 69.2 MiB | 0.64-0.83 s (69.4-90.0 MiB/s) | 0.97-1.15 s (50.2-59.6 MiB/s) | 0.534-1.489 s | 0 B | 49 % / 0.06 GiB | 49 % / 0.00 GiB |
| tar.gz | No | hot | 55.1 MiB | 1.87-1.92 s (30.2-30.9 MiB/s) | 0.69-0.79 s (73.4-84.3 MiB/s) | 0.257-0.265 s | 0 B | 42 % / 0.01 GiB | 54 % / 0.01 GiB |
| tar.gz | Yes | hot | 55.1 MiB | 2.24-3.17 s (18.3-25.9 MiB/s) | 1.06-2.41 s (24.0-54.8 MiB/s) | 1.621-1.740 s | 0 B | 40 % / 0.02 GiB | 41 % / 0.01 GiB |
| zarlib | No | cold | 23.6 MiB | 0.55-0.55 s (105.0-105.0 MiB/s) | 0.80-0.80 s (72.6-72.6 MiB/s) | 0.099-0.099 s | 0 B | 38 % / 0.23 GiB | 46 % / 0.15 GiB |
| zarlib | Yes | cold | 23.6 MiB | 0.68-0.68 s (85.5-85.5 MiB/s) | 0.89-0.89 s (65.0-65.0 MiB/s) | 0.263-0.263 s | 0 B | 48 % / 0.26 GiB | 39 % / 0.17 GiB |
| zip | No | cold | 60.9 MiB | 4.72-4.72 s (12.2-12.2 MiB/s) | 1.74-1.74 s (33.3-33.3 MiB/s) | 0.013-0.013 s | 0 B | 57 % / 0.01 GiB | 59 % / 0.00 GiB |
| zip | Yes | cold | 61.3 MiB | 5.04-5.04 s (11.5-11.5 MiB/s) | 2.51-2.51 s (23.0-23.0 MiB/s) | 0.038-0.038 s | 0 B | 54 % / 0.01 GiB | 63 % / 0.00 GiB |
| 7z | No | cold | 22.0 MiB | 8.81-8.81 s (6.6-6.6 MiB/s) | 1.92-1.92 s (30.1-30.1 MiB/s) | 0.649-0.649 s | 0 B | 46 % / 0.24 GiB | 45 % / 0.07 GiB |
| 7z | Yes | cold | 22.0 MiB | 10.17-10.17 s (5.7-5.7 MiB/s) | 2.07-2.07 s (28.0-28.0 MiB/s) | 0.661-0.661 s | 0 B | 50 % / 0.27 GiB | 48 % / 0.07 GiB |
| tar | No | cold | 69.2 MiB | 2.34-2.34 s (24.8-24.8 MiB/s) | 0.75-0.75 s (76.7-76.7 MiB/s) | 0.076-0.076 s | 0 B | 45 % / 0.00 GiB | 30 % / 0.00 GiB |
| tar | Yes | cold | 69.2 MiB | 4.28-4.28 s (13.5-13.5 MiB/s) | 2.51-2.51 s (23.0-23.0 MiB/s) | 1.954-1.954 s | 0 B | 34 % / 0.01 GiB | 39 % / 0.01 GiB |
| tar.gz | No | cold | 55.1 MiB | 2.70-2.70 s (21.4-21.4 MiB/s) | 0.77-0.77 s (75.5-75.5 MiB/s) | 0.282-0.282 s | 0 B | 38 % / 0.01 GiB | 35 % / 0.01 GiB |
| tar.gz | Yes | cold | 55.1 MiB | 4.07-4.07 s (14.2-14.2 MiB/s) | 2.30-2.30 s (25.2-25.2 MiB/s) | 1.879-1.879 s | 0 B | 37 % / 0.01 GiB | 37 % / 0.01 GiB |



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
