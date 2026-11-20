# linux-4.0.1 Archive Benchmark

Run date: 2026-11-20T07:27:08+01:00
- Operating system: Linux-5.15.0-198-generic-x86_64-with-glibc2.35
- CPU: Intel(R) Core(TM) i7-10700 CPU @ 2.90GHz (x86_64, 16 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-core-i7-linux-4.0.1.svg)

## Results

Source: [`linux-4.0.1.zar`](../../testdata/linux-4.0.1.zar)
Dataset: Linux 4.0.1 source tree from a ZAR archive
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 115.3 MiB | 0.98-1.03 s (511.1-534.1 MiB/s) | 0.89-0.91 s (577.3-588.4 MiB/s) | 0.108-0.118 s | 0 B | 0 % / 0.71 GiB | 51 % / 0.50 GiB |
| zarlib | Yes | hot | 115.3 MiB | 1.08-1.14 s (462.0-486.5 MiB/s) | 1.17-1.22 s (431.8-449.8 MiB/s) | 0.218-0.218 s | 0 B | 44 % / 0.45 GiB | 55 % / 0.78 GiB |
| zip | No | hot | 150.3 MiB | 11.86-11.90 s (44.1-44.3 MiB/s) | 4.23-4.35 s (120.7-124.0 MiB/s) | 0.007-0.008 s | 0 B | 14 % / 0.03 GiB | 13 % / 0.00 GiB |
| zip | Yes | hot | 151.6 MiB | 12.31-12.43 s (42.2-42.7 MiB/s) | 5.41-5.43 s (96.6-97.1 MiB/s) | 0.015-0.015 s | 0 B | 18 % / 0.03 GiB | 12 % / 0.00 GiB |
| 7z | No | hot | 81.1 MiB | 26.86-27.61 s (19.0-19.5 MiB/s) | 6.11-6.15 s (85.4-85.9 MiB/s) | 3.794-3.806 s | 0 B | 68 % / 1.53 GiB | 12 % / 0.03 GiB |
| 7z | Yes | hot | 81.1 MiB | 25.94-26.03 s (20.2-20.2 MiB/s) | 6.24-6.26 s (83.9-84.1 MiB/s) | 3.941-4.057 s | 0 B | 68 % / 1.59 GiB | 11 % / 0.03 GiB |
| tar | No | hot | 562.7 MiB | 0.63-0.65 s (807.6-839.9 MiB/s) | 0.95-0.98 s (538.4-553.2 MiB/s) | 0.069-0.070 s | 0 B | 40 % / 0.00 GiB | 13 % / 0.00 GiB |
| tar | Yes | hot | 562.7 MiB | 2.27-2.46 s (213.8-231.4 MiB/s) | 2.20-2.45 s (214.5-239.1 MiB/s) | 1.598-1.619 s | 0 B | 10 % / 0.45 GiB | 9 % / 0.27 GiB |
| tar.gz | No | hot | 122.2 MiB | 12.99-13.14 s (40.0-40.4 MiB/s) | 2.30-2.39 s (219.6-228.1 MiB/s) | 2.251-2.286 s | 0 B | 9 % / 0.01 GiB | 14 % / 0.00 GiB |
| tar.gz | Yes | hot | 122.2 MiB | 13.40-13.44 s (39.1-39.2 MiB/s) | 2.61-2.65 s (197.8-201.2 MiB/s) | 2.594-2.618 s | 0 B | 9 % / 0.01 GiB | 11 % / 0.00 GiB |
| zarlib | No | cold | 115.3 MiB | 1.09-1.09 s (481.5-481.5 MiB/s) | 0.92-0.92 s (573.1-573.1 MiB/s) | 0.151-0.151 s | 0 B | 12 % / 0.70 GiB | 12 % / 0.18 GiB |
| zarlib | Yes | cold | 115.4 MiB | 1.19-1.19 s (442.8-442.8 MiB/s) | 1.34-1.34 s (390.5-390.5 MiB/s) | 0.247-0.247 s | 0 B | 60 % / 0.53 GiB | 41 % / 0.77 GiB |
| zip | No | cold | 150.3 MiB | 14.85-14.85 s (35.4-35.4 MiB/s) | 4.27-4.27 s (123.1-123.1 MiB/s) | 0.013-0.013 s | 0 B | 11 % / 0.03 GiB | 13 % / 0.00 GiB |
| zip | Yes | cold | 151.6 MiB | 15.59-15.59 s (33.7-33.7 MiB/s) | 5.50-5.50 s (95.5-95.5 MiB/s) | 0.027-0.027 s | 0 B | 17 % / 0.03 GiB | 12 % / 0.00 GiB |
| 7z | No | cold | 81.1 MiB | 30.21-30.21 s (17.4-17.4 MiB/s) | 6.21-6.21 s (84.5-84.5 MiB/s) | 3.967-3.967 s | 0 B | 66 % / 1.47 GiB | 11 % / 0.03 GiB |
| 7z | Yes | cold | 81.1 MiB | 28.95-28.95 s (18.1-18.1 MiB/s) | 6.30-6.30 s (83.4-83.4 MiB/s) | 4.003-4.003 s | 0 B | 62 % / 1.46 GiB | 12 % / 0.03 GiB |
| tar | No | cold | 562.7 MiB | 4.18-4.18 s (125.8-125.8 MiB/s) | 1.10-1.10 s (478.3-478.3 MiB/s) | 0.383-0.383 s | 0 B | 14 % / 0.00 GiB | 10 % / 0.00 GiB |
| tar | Yes | cold | 562.7 MiB | 5.92-5.92 s (88.6-88.6 MiB/s) | 2.61-2.61 s (201.4-201.4 MiB/s) | 1.761-1.761 s | 0 B | 8 % / 0.17 GiB | 8 % / 0.22 GiB |
| tar.gz | No | cold | 122.2 MiB | 13.91-13.91 s (37.8-37.8 MiB/s) | 2.47-2.47 s (212.2-212.2 MiB/s) | 2.340-2.340 s | 0 B | 9 % / 0.01 GiB | 14 % / 0.00 GiB |
| tar.gz | Yes | cold | 122.2 MiB | 13.50-13.50 s (38.9-38.9 MiB/s) | 2.76-2.76 s (190.1-190.1 MiB/s) | 2.694-2.694 s | 0 B | 9 % / 0.01 GiB | 12 % / 0.00 GiB |



"Extract 3 files" is the time to extract only `fs/btrfs/check-integrity.c`, `lib/xz/xz_dec_test.c`, `tools/perf/util/build-id.h`
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
