# Linux 4.0.1 Archive Benchmark

Run date: 2026-10-31T19:34:49+01:00  
- Operating system: Linux-7.2.6-070206-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, extraction time, CPU, and RAM charts](./benchmark-charts-linux-x86_64.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)  
Uncompressed source size: 525.1 MiB  
Regular files verified: 48,945  
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 151.3 MiB | 1.73-1.80 s (291.3-303.3 MiB/s) | 2.36-2.40 s (219.1-222.2 MiB/s) | 0.905-0.909 s | 0 B | 13 % / 7.3 GiB | 45 % / 7.3 GiB |
| zarlib | Yes | hot | 152.6 MiB | 2.33-2.39 s (219.2-225.8 MiB/s) | 3.47-3.96 s (132.6-151.2 MiB/s) | 1.791-1.849 s | 0 B | 57 % / 7.7 GiB | 45 % / 7.8 GiB |
| zip | No | hot | 151.1 MiB | 15.51-15.60 s (33.7-33.9 MiB/s) | 6.14-6.30 s (83.3-85.5 MiB/s) | 0.017-0.021 s | 0 B | 17 % / 7.3 GiB | 15 % / 7.1 GiB |
| zip | Yes | hot | 152.4 MiB | 16.35-16.87 s (31.1-32.1 MiB/s) | 7.62-8.14 s (64.5-68.9 MiB/s) | 0.033-0.040 s | 0 B | 14 % / 7.1 GiB | 28 % / 7.0 GiB |
| 7z | No | hot | 80.2 MiB | 63.14-64.00 s (8.2-8.3 MiB/s) | 3.97-3.99 s (131.5-132.3 MiB/s) | 1.235-1.275 s | 0 B | 71 % / 8.7 GiB | 23 % / 7.6 GiB |
| 7z | Yes | hot | 80.2 MiB | 62.61-63.44 s (8.3-8.4 MiB/s) | 4.07-4.12 s (127.5-129.2 MiB/s) | 1.296-1.319 s | 0 B | 72 % / 8.7 GiB | 23 % / 7.6 GiB |
| tar | No | hot | 562.7 MiB | 0.99-1.00 s (522.6-527.8 MiB/s) | 2.01-2.05 s (256.3-261.7 MiB/s) | 0.095-0.110 s | 0 B | 22 % / 7.2 GiB | 17 % / 7.1 GiB |
| tar | Yes | hot | 562.7 MiB | 4.05-12.94 s (40.6-129.7 MiB/s) | 4.53-14.90 s (35.2-115.9 MiB/s) | 9.361-17.315 s | 0 B | 8 % / 7.6 GiB | 8 % / 7.5 GiB |
| tar.gz | No | hot | 122.2 MiB | 15.18-15.36 s (34.2-34.6 MiB/s) | 2.28-2.29 s (228.8-230.6 MiB/s) | 1.574-1.793 s | 0 B | 17 % / 7.3 GiB | 21 % / 7.4 GiB |
| tar.gz | Yes | hot | 122.2 MiB | 15.88-15.92 s (33.0-33.1 MiB/s) | 2.88-3.11 s (168.8-182.5 MiB/s) | 2.379-2.435 s | 0 B | 18 % / 7.1 GiB | 25 % / 7.6 GiB |
| zarlib | No | cold | 151.3 MiB | 2.30-2.30 s (228.1-228.1 MiB/s) | 2.57-2.57 s (204.1-204.1 MiB/s) | 0.930-0.930 s | 0 B | 54 % / 6.9 GiB | 36 % / 7.0 GiB |
| zarlib | Yes | cold | 152.6 MiB | 2.23-2.23 s (235.2-235.2 MiB/s) | 3.41-3.41 s (154.2-154.2 MiB/s) | 1.938-1.938 s | 0 B | 56 % / 6.8 GiB | 32 % / 7.0 GiB |
| zip | No | cold | 151.1 MiB | 21.08-21.08 s (24.9-24.9 MiB/s) | 6.29-6.29 s (83.4-83.4 MiB/s) | 0.023-0.023 s | 0 B | 13 % / 6.8 GiB | 18 % / 6.7 GiB |
| zip | Yes | cold | 152.4 MiB | 22.76-22.76 s (23.1-23.1 MiB/s) | 7.99-7.99 s (65.7-65.7 MiB/s) | 0.042-0.042 s | 0 B | 20 % / 7.0 GiB | 23 % / 7.0 GiB |
| 7z | No | cold | 80.2 MiB | 71.54-71.54 s (7.3-7.3 MiB/s) | 4.94-4.94 s (106.3-106.3 MiB/s) | 1.372-1.372 s | 0 B | 75 % / 8.9 GiB | 30 % / 8.0 GiB |
| 7z | Yes | cold | 80.2 MiB | 67.46-67.46 s (7.8-7.8 MiB/s) | 4.48-4.48 s (117.2-117.2 MiB/s) | 1.707-1.707 s | 0 B | 70 % / 9.3 GiB | 18 % / 8.2 GiB |
| tar | No | cold | 562.7 MiB | 8.22-8.22 s (63.9-63.9 MiB/s) | 2.96-2.96 s (177.2-177.2 MiB/s) | 0.484-0.484 s | 0 B | 44 % / 8.4 GiB | 19 % / 10.3 GiB |
| tar | Yes | cold | 562.7 MiB | 22.51-22.51 s (23.3-23.3 MiB/s) | 17.94-17.94 s (29.3-29.3 MiB/s) | 15.482-15.482 s | 0 B | 7 % / 10.2 GiB | 7 % / 10.0 GiB |
| tar.gz | No | cold | 122.2 MiB | 15.66-15.66 s (33.5-33.5 MiB/s) | 2.96-2.96 s (177.6-177.6 MiB/s) | 1.652-1.652 s | 0 B | 19 % / 10.2 GiB | 14 % / 10.2 GiB |
| tar.gz | Yes | cold | 122.2 MiB | 19.37-19.37 s (27.1-27.1 MiB/s) | 6.16-6.16 s (85.2-85.2 MiB/s) | 5.372-5.372 s | 0 B | 16 % / 10.0 GiB | 13 % / 10.3 GiB |

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

- `zarlib`: `zarc` release build, default Zstandard compression level 3;
  encrypted runs use AES-256-GCM and the default header-and-data scope.
- `zip`: Info-ZIP recursive DEFLATE at level 6, storing symlinks as links.
  Encrypted runs use the implementation's standard ZipCrypto mode
  (legacy/weak encryption).
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
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
