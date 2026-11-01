# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-01T12:36:04+01:00
- Operating system: Linux-7.2.6-070206-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, extraction time, CPU, and RAM charts](./benchmark-charts-linux-x86_64-ryzen5.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 151.3 MiB | 3.86-4.42 s (118.9-136.0 MiB/s) | 4.03-4.45 s (117.9-130.3 MiB/s) | 0.409-0.470 s | 0 B | 65 % / 27.5 GiB | 74 % / 27.4 GiB |
| zarlib | Yes | hot | 152.6 MiB | 4.33-4.61 s (113.9-121.3 MiB/s) | 4.83-4.97 s (105.6-108.7 MiB/s) | 0.976-1.128 s | 0 B | 78 % / 27.5 GiB | 60 % / 27.6 GiB |
| zip | No | hot | 151.1 MiB | 17.38-17.76 s (29.6-30.2 MiB/s) | 9.42-10.24 s (51.3-55.8 MiB/s) | 0.013-0.023 s | 0 B | 48 % / 27.6 GiB | 89 % / 27.8 GiB |
| zip | Yes | hot | 152.4 MiB | 18.28-18.42 s (28.5-28.7 MiB/s) | 11.60-11.73 s (44.8-45.3 MiB/s) | 0.037-0.099 s | 0 B | 46 % / 27.4 GiB | 89 % / 27.5 GiB |
| 7z | No | hot | 80.2 MiB | 71.68-74.76 s (7.0-7.3 MiB/s) | 5.91-6.24 s (84.1-88.9 MiB/s) | 1.655-1.718 s | 0 B | 82 % / 29.0 GiB | 79 % / 27.9 GiB |
| 7z | Yes | hot | 80.2 MiB | 69.88-70.57 s (7.4-7.5 MiB/s) | 5.78-5.90 s (89.1-90.8 MiB/s) | 1.831-1.898 s | 0 B | 79 % / 28.9 GiB | 72 % / 27.8 GiB |
| tar | No | hot | 562.7 MiB | 1.61-1.64 s (321.1-326.5 MiB/s) | 2.18-2.25 s (233.1-241.1 MiB/s) | 0.101-0.115 s | 0 B | 59 % / 27.2 GiB | 44 % / 27.2 GiB |
| tar | Yes | hot | 562.7 MiB | 5.49-17.93 s (29.3-95.6 MiB/s) | 13.34-19.26 s (27.3-39.4 MiB/s) | 16.991-17.681 s | 0 B | 22 % / 27.3 GiB | 23 % / 27.3 GiB |
| tar.gz | No | hot | 122.2 MiB | 15.34-15.40 s (34.1-34.2 MiB/s) | 2.87-3.24 s (162.1-182.8 MiB/s) | 1.653-1.886 s | 0 B | 34 % / 27.4 GiB | 47 % / 27.6 GiB |
| tar.gz | Yes | hot | 122.2 MiB | 16.51-16.56 s (31.7-31.8 MiB/s) | 3.81-4.30 s (122.2-137.9 MiB/s) | 2.568-2.706 s | 0 B | 34 % / 27.4 GiB | 63 % / 27.7 GiB |
| zarlib | No | cold | 151.3 MiB | 4.14-4.14 s (126.7-126.7 MiB/s) | 3.77-3.77 s (139.4-139.4 MiB/s) | 0.543-0.543 s | 0 B | 74 % / 27.5 GiB | 58 % / 27.6 GiB |
| zarlib | Yes | cold | 152.6 MiB | 5.95-5.95 s (88.2-88.2 MiB/s) | 4.90-4.90 s (107.0-107.0 MiB/s) | 1.318-1.318 s | 0 B | 87 % / 27.5 GiB | 62 % / 27.4 GiB |
| zip | No | cold | 151.1 MiB | 27.94-27.94 s (18.8-18.8 MiB/s) | 10.00-10.00 s (52.5-52.5 MiB/s) | 0.020-0.020 s | 0 B | 47 % / 28.3 GiB | 85 % / 28.1 GiB |
| zip | Yes | cold | 152.4 MiB | 28.11-28.11 s (18.7-18.7 MiB/s) | 11.75-11.75 s (44.7-44.7 MiB/s) | 0.044-0.044 s | 0 B | 45 % / 28.1 GiB | 85 % / 28.0 GiB |
| 7z | No | cold | 80.2 MiB | 89.48-89.48 s (5.9-5.9 MiB/s) | 7.09-7.09 s (74.0-74.0 MiB/s) | 1.888-1.888 s | 0 B | 88 % / 29.4 GiB | 78 % / 28.3 GiB |
| 7z | Yes | cold | 80.2 MiB | 93.87-93.87 s (5.6-5.6 MiB/s) | 8.00-8.00 s (65.6-65.6 MiB/s) | 1.832-1.832 s | 0 B | 89 % / 29.6 GiB | 88 % / 28.6 GiB |
| tar | No | cold | 562.7 MiB | 9.13-9.13 s (57.5-57.5 MiB/s) | 2.62-2.62 s (200.7-200.7 MiB/s) | 0.515-0.515 s | 0 B | 75 % / 28.1 GiB | 36 % / 28.0 GiB |
| tar | Yes | cold | 562.7 MiB | 12.45-12.45 s (42.2-42.2 MiB/s) | 10.59-10.59 s (49.6-49.6 MiB/s) | 16.651-16.651 s | 0 B | 40 % / 28.1 GiB | 30 % / 28.9 GiB |
| tar.gz | No | cold | 122.2 MiB | 17.60-17.60 s (29.8-29.8 MiB/s) | 4.15-4.15 s (126.7-126.7 MiB/s) | 1.931-1.931 s | 0 B | 46 % / 28.1 GiB | 63 % / 27.7 GiB |
| tar.gz | Yes | cold | 122.2 MiB | 21.34-21.34 s (24.6-24.6 MiB/s) | 6.07-6.07 s (86.5-86.5 MiB/s) | 5.177-5.177 s | 0 B | 46 % / 28.0 GiB | 33 % / 28.3 GiB |

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
the minimum and maximum measured time for each phase. The time-series
charts below the bar charts plot every sample, with one line per archive format
(solid = plain, dashed = encrypted; thick line = CPU %, thin dashed line = RAM GiB).
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
  maximum runtime across those runs. Build time, source download/extraction, and
  post-extraction verification are excluded from the measured operation times.

## Tool versions

- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.8.0`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
