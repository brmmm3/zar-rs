# duplicates Archive Benchmark

Run date: 2026-11-20T07:16:03+01:00
- Operating system: Linux-5.15.0-198-generic-x86_64-with-glibc2.35
- CPU: Intel(R) Core(TM) i7-10700 CPU @ 2.90GHz (x86_64, 16 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-core-i7-duplicates.svg)

## Results

Source: [`duplicates.zar`](../../testdata/duplicates.zar)
Dataset: duplicate icon tree from a ZAR archive
Uncompressed source size: 57.9 MiB
Regular files verified: 15,383
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.6 MiB | 0.15-0.17 s (343.5-378.5 MiB/s) | 0.21-0.23 s (247.9-280.9 MiB/s) | 0.034-0.036 s | 0 B | N/A | N/A |
| zarlib | Yes | hot | 23.6 MiB | 0.22-0.23 s (251.3-258.1 MiB/s) | 0.34-0.36 s (160.0-172.5 MiB/s) | 0.128-0.133 s | 0 B | N/A | N/A |
| zip | No | hot | 60.9 MiB | 1.42-1.43 s (40.5-40.6 MiB/s) | 0.62-0.63 s (91.4-92.6 MiB/s) | 0.004-0.004 s | 0 B | 9 % / 0.01 GiB | 15 % / 0.00 GiB |
| zip | Yes | hot | 61.3 MiB | 1.62-1.65 s (35.0-35.6 MiB/s) | 1.11-1.12 s (51.7-52.1 MiB/s) | 0.008-0.009 s | 0 B | 16 % / 0.01 GiB | 15 % / 0.00 GiB |
| 7z | No | hot | 43.1 MiB | 5.38-5.48 s (10.6-10.8 MiB/s) | 2.95-2.96 s (19.5-19.6 MiB/s) | 0.878-0.893 s | 0 B | 14 % / 0.14 GiB | 10 % / 0.02 GiB |
| 7z | Yes | hot | 43.1 MiB | 5.45-5.60 s (10.3-10.6 MiB/s) | 3.11-3.24 s (17.9-18.6 MiB/s) | 1.013-1.014 s | 0 B | 15 % / 0.14 GiB | 15 % / 0.02 GiB |
| tar | No | hot | 69.2 MiB | 0.12-0.13 s (428.7-487.0 MiB/s) | 0.23-0.23 s (249.3-250.0 MiB/s) | 0.019-0.021 s | 0 B | N/A | N/A |
| tar | Yes | hot | 69.2 MiB | 0.32-0.38 s (153.0-178.1 MiB/s) | 0.40-0.44 s (130.6-146.2 MiB/s) | 0.217-0.218 s | 0 B | N/A | N/A |
| tar.gz | No | hot | 55.0 MiB | 1.46-1.50 s (38.5-39.5 MiB/s) | 0.39-0.40 s (144.6-147.7 MiB/s) | 0.332-0.338 s | 0 B | 11 % / 0.01 GiB | N/A |
| tar.gz | Yes | hot | 55.0 MiB | 1.64-1.67 s (34.6-35.2 MiB/s) | 0.51-0.55 s (104.3-112.5 MiB/s) | 0.489-0.502 s | 0 B | 11 % / 0.02 GiB | 12 % / 0.00 GiB |
| zarlib | No | cold | 23.6 MiB | 0.21-0.21 s (274.9-274.9 MiB/s) | 0.25-0.25 s (231.7-231.7 MiB/s) | 0.051-0.051 s | 0 B | N/A | N/A |
| zarlib | Yes | cold | 23.6 MiB | 0.29-0.29 s (198.9-198.9 MiB/s) | 0.36-0.36 s (160.7-160.7 MiB/s) | 0.145-0.145 s | 0 B | N/A | N/A |
| zip | No | cold | 60.9 MiB | 2.44-2.44 s (23.7-23.7 MiB/s) | 0.65-0.65 s (89.6-89.6 MiB/s) | 0.012-0.012 s | 0 B | 18 % / 0.01 GiB | 9 % / 0.00 GiB |
| zip | Yes | cold | 61.3 MiB | 2.50-2.50 s (23.2-23.2 MiB/s) | 1.11-1.11 s (52.3-52.3 MiB/s) | 0.015-0.015 s | 0 B | 11 % / 0.01 GiB | 11 % / 0.00 GiB |
| 7z | No | cold | 43.1 MiB | 5.49-5.49 s (10.5-10.5 MiB/s) | 3.11-3.11 s (18.6-18.6 MiB/s) | 0.890-0.890 s | 0 B | 15 % / 0.14 GiB | 15 % / 0.02 GiB |
| 7z | Yes | cold | 43.1 MiB | 5.63-5.63 s (10.3-10.3 MiB/s) | 3.25-3.25 s (17.8-17.8 MiB/s) | 1.031-1.031 s | 0 B | 16 % / 0.13 GiB | 16 % / 0.02 GiB |
| tar | No | cold | 69.2 MiB | 1.04-1.04 s (55.9-55.9 MiB/s) | 0.26-0.26 s (222.9-222.9 MiB/s) | 0.054-0.054 s | 0 B | 14 % / 0.00 GiB | N/A |
| tar | Yes | cold | 69.2 MiB | 1.26-1.26 s (46.0-46.0 MiB/s) | 0.48-0.48 s (121.3-121.3 MiB/s) | 0.238-0.238 s | 0 B | 5 % / 0.01 GiB | N/A |
| tar.gz | No | cold | 55.0 MiB | 1.68-1.68 s (34.3-34.3 MiB/s) | 0.43-0.43 s (135.8-135.8 MiB/s) | 0.374-0.374 s | 0 B | 10 % / 0.01 GiB | N/A |
| tar.gz | Yes | cold | 55.0 MiB | 1.86-1.86 s (31.0-31.0 MiB/s) | 0.57-0.57 s (101.7-101.7 MiB/s) | 0.531-0.531 s | 0 B | 10 % / 0.01 GiB | 10 % / 0.00 GiB |



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
