# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-01T16:45:06+01:00
- Operating system: Linux-5.15.0-198-generic-x86_64-with-glibc2.35
- CPU: Intel(R) Core(TM) i7-10700 CPU @ 2.90GHz (x86_64, 16 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Not reported; device: dm-2
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Not reported; device: dm-2

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-core-i7.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 151.3 MiB | 0.92-0.92 s (568.5-569.2 MiB/s) | 1.26-1.27 s (414.7-416.7 MiB/s) | 0.189-0.192 s | 0 B | 0 % / 0.0 GiB | 0 % / 9.2 GiB |
| zarlib | Yes | hot | 152.6 MiB | 1.08-1.09 s (481.3-486.7 MiB/s) | 1.70-1.70 s (308.6-309.0 MiB/s) | 0.399-0.405 s | 0 B | 0 % / 9.3 GiB | 22 % / 9.2 GiB |
| zip | No | hot | 151.1 MiB | 11.13-11.16 s (47.0-47.2 MiB/s) | 3.86-3.91 s (134.4-136.1 MiB/s) | 0.007-0.007 s | 0 B | 7 % / 9.2 GiB | 6 % / 9.1 GiB |
| zip | Yes | hot | 152.4 MiB | 11.58-11.71 s (44.8-45.4 MiB/s) | 4.94-4.96 s (105.9-106.2 MiB/s) | 0.012-0.012 s | 0 B | 6 % / 9.2 GiB | 7 % / 9.2 GiB |
| 7z | No | hot | 81.1 MiB | 24.86-24.89 s (21.1-21.1 MiB/s) | 5.66-5.72 s (91.7-92.7 MiB/s) | 3.416-3.418 s | 0 B | 65 % / 10.7 GiB | 6 % / 9.1 GiB |
| 7z | Yes | hot | 81.1 MiB | 24.73-25.10 s (20.9-21.2 MiB/s) | 5.82-5.91 s (88.9-90.2 MiB/s) | 3.558-3.575 s | 0 B | 63 % / 10.7 GiB | 7 % / 9.2 GiB |
| tar | No | hot | 562.7 MiB | 0.53-0.53 s (998.5-998.8 MiB/s) | 0.92-0.92 s (569.1-573.5 MiB/s) | 0.063-0.063 s | 0 B | 0 % / 0.0 GiB | 0 % / 0.0 GiB |
| tar | Yes | hot | 562.7 MiB | 2.05-2.31 s (226.9-256.6 MiB/s) | 2.07-2.29 s (229.1-253.1 MiB/s) | 1.396-1.765 s | 0 B | 3 % / 9.7 GiB | 8 % / 9.5 GiB |
| tar.gz | No | hot | 122.1 MiB | 12.63-13.21 s (39.8-41.6 MiB/s) | 2.22-2.23 s (235.8-237.0 MiB/s) | 2.186-2.188 s | 0 B | 7 % / 9.2 GiB | 9 % / 9.2 GiB |
| tar.gz | Yes | hot | 122.1 MiB | 13.15-13.26 s (39.6-39.9 MiB/s) | 2.48-2.54 s (206.9-211.5 MiB/s) | 2.479-2.490 s | 0 B | 8 % / 9.2 GiB | 9 % / 9.2 GiB |
| zarlib | No | cold | 151.3 MiB | 1.06-1.06 s (496.7-496.7 MiB/s) | 1.43-1.43 s (367.4-367.4 MiB/s) | 0.341-0.341 s | 0 B | 9 % / 9.2 GiB | 14 % / 9.1 GiB |
| zarlib | Yes | cold | 152.6 MiB | 1.21-1.21 s (435.7-435.7 MiB/s) | 1.87-1.87 s (281.4-281.4 MiB/s) | 0.535-0.535 s | 0 B | 13 % / 9.2 GiB | 17 % / 9.1 GiB |
| zip | No | cold | 151.1 MiB | 17.15-17.15 s (30.6-30.6 MiB/s) | 4.23-4.23 s (124.1-124.1 MiB/s) | 0.015-0.015 s | 0 B | 5 % / 9.1 GiB | 8 % / 9.0 GiB |
| zip | Yes | cold | 152.4 MiB | 17.60-17.60 s (29.8-29.8 MiB/s) | 5.30-5.30 s (99.0-99.0 MiB/s) | 0.020-0.020 s | 0 B | 5 % / 9.1 GiB | 7 % / 9.0 GiB |
| 7z | No | cold | 81.1 MiB | 26.48-26.48 s (19.8-19.8 MiB/s) | 5.83-5.83 s (90.0-90.0 MiB/s) | 3.487-3.487 s | 0 B | 60 % / 10.5 GiB | 7 % / 9.0 GiB |
| 7z | Yes | cold | 81.1 MiB | 26.98-26.98 s (19.5-19.5 MiB/s) | 5.89-5.89 s (89.1-89.1 MiB/s) | 3.577-3.577 s | 0 B | 61 % / 10.5 GiB | 7 % / 9.0 GiB |
| tar | No | cold | 562.7 MiB | 6.99-6.99 s (75.1-75.1 MiB/s) | 1.36-1.36 s (386.8-386.8 MiB/s) | 0.840-0.840 s | 0 B | 4 % / 9.0 GiB | 5 % / 9.0 GiB |
| tar | Yes | cold | 562.7 MiB | 10.85-10.85 s (48.4-48.4 MiB/s) | 3.11-3.11 s (168.9-168.9 MiB/s) | 2.870-2.870 s | 0 B | 4 % / 9.1 GiB | 11 % / 9.5 GiB |
| tar.gz | No | cold | 122.1 MiB | 13.44-13.44 s (39.1-39.1 MiB/s) | 2.36-2.36 s (222.9-222.9 MiB/s) | 2.290-2.290 s | 0 B | 7 % / 9.0 GiB | 8 % / 9.0 GiB |
| tar.gz | Yes | cold | 122.1 MiB | 14.03-14.03 s (37.4-37.4 MiB/s) | 3.39-3.39 s (155.1-155.1 MiB/s) | 3.214-3.214 s | 0 B | 7 % / 9.0 GiB | 8 % / 9.0 GiB |

"Extract 3 files" is the time to extract only `fs/btrfs/check-integrity.c`, `lib/xz/xz_dec_test.c`, `tools/perf/util/build-id.h`
from the archive (for encrypted tar/tar.gz this includes decrypting the archive). "Temp space"
is the peak disk usage of the results directory and the runner's temporary
directory during that extraction minus the size of the extracted files, sampled
every 5 ms (the maximum over all runs). Temporary files outside these
directories (e.g. in the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 4 extraction run(s), `zarlib` renamed 48 path(s) total (12 per run) due to case-folded collisions; generated suffixes are normalized only for comparison. The results
directory was emptied again after each check. Symlink entries are excluded
because `zarlib` currently reports and skips unsupported input symlinks. ZIP
and tar preserve links; 7z preserves them in its archive but may skip unsafe
relative links during extraction, as noted above.

CPU utilisation and RAM consumption were sampled at 1-second intervals during
each compression and extraction operation using `psutil`. The table shows the
average CPU % and RAM GiB across all samples for each phase. Runtime values show
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
  maximum runtime across those runs. The runner waits 3 seconds between
  benchmark runs; this delay is not included in the measured operation times.
  Build time, source download/extraction, and post-extraction verification are
  also excluded from the measured operation times.

## Tool versions

- `zarlib`: `0.8.0`
- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.34`
- `openssl`: `OpenSSL 3.0.2 15 Mar 2022 (Library: OpenSSL 3.0.2 15 Mar 2022)`
- `7z`: `7-Zip [64] 16.02 : Copyright (c) 1999-2016 Igor Pavlov : 2016-05-21`
- `gzip`: `gzip 1.10`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.8.0`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
