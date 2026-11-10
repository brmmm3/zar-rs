# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-08T22:05:03+01:00

- Operating system: Linux-7.2.6-070206-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be
  virtual); model: Samsung SSD 980 1TB; device: nvme1n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual);
  model: Samsung SSD 980 1TB; device: nvme1n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-ryzen5.svg)

## Results

Source:
[`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Uncompressed source size: 525.1 MiB Regular files verified: 48,945 Symbolic
links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 115.0 MiB | 3.31-3.59 s (146.5-158.8 MiB/s) | 2.96-3.44 s (152.7-177.3 MiB/s) | 0.349-0.388 s | 0 B | 78 % / 18.1 GiB | 81 % / 18.0 GiB |
| zarlib | Yes | hot | 115.0 MiB | 3.68-3.70 s (141.7-142.6 MiB/s) | 4.21-4.52 s (116.2-124.8 MiB/s) | 0.843-1.145 s | 0 B | 90 % / 18.2 GiB | 79 % / 18.2 GiB |
| zip | No | hot | 151.1 MiB | 19.65-20.03 s (26.2-26.7 MiB/s) | 13.37-14.07 s (37.3-39.3 MiB/s) | 0.018-0.024 s | 0 B | 56 % / 17.5 GiB | 91 % / 17.6 GiB |
| zip | Yes | hot | 152.4 MiB | 17.95-18.18 s (28.9-29.3 MiB/s) | 12.81-13.55 s (38.8-41.0 MiB/s) | 0.040-0.054 s | 0 B | 41 % / 17.4 GiB | 90 % / 17.4 GiB |
| 7z | No | hot | 80.2 MiB | 66.09-67.89 s (7.7-7.9 MiB/s) | 6.92-6.94 s (75.7-75.9 MiB/s) | 1.787-1.830 s | 0 B | 78 % / 18.9 GiB | 78 % / 17.8 GiB |
| 7z | Yes | hot | 80.2 MiB | 66.24-66.62 s (7.9-7.9 MiB/s) | 6.62-7.07 s (74.3-79.3 MiB/s) | 1.922-2.123 s | 0 B | 77 % / 18.8 GiB | 81 % / 17.6 GiB |
| tar | No | hot | 562.7 MiB | 1.69-1.73 s (303.0-311.1 MiB/s) | 2.26-2.26 s (231.8-231.9 MiB/s) | 0.114-0.130 s | 0 B | 26 % / 17.1 GiB | 39 % / 17.1 GiB |
| tar | Yes | hot | 562.7 MiB | 4.66-16.01 s (32.8-112.7 MiB/s) | 5.13-18.85 s (27.9-102.3 MiB/s) | 11.291-17.339 s | 0 B | 21 % / 17.5 GiB | 11 % / 17.9 GiB |
| tar.gz | No | hot | 122.2 MiB | 14.94-14.96 s (35.1-35.2 MiB/s) | 2.43-3.05 s (172.3-215.9 MiB/s) | 1.607-1.633 s | 0 B | 26 % / 17.3 GiB | 34 % / 17.5 GiB |
| tar.gz | Yes | hot | 122.2 MiB | 16.11-16.60 s (31.6-32.6 MiB/s) | 3.17-3.29 s (159.7-165.5 MiB/s) | 2.629-2.690 s | 0 B | 29 % / 17.3 GiB | 36 % / 17.7 GiB |
| zarlib | No | cold | 115.2 MiB | 2.93-2.93 s (179.1-179.1 MiB/s) | 2.85-2.85 s (184.1-184.1 MiB/s) | 0.460-0.460 s | 0 B | 65 % / 17.2 GiB | 69 % / 17.2 GiB |
| zarlib | Yes | cold | 115.3 MiB | 4.20-4.20 s (125.0-125.0 MiB/s) | 3.87-3.87 s (135.6-135.6 MiB/s) | 1.029-1.029 s | 0 B | 88 % / 17.4 GiB | 65 % / 17.3 GiB |
| zip | No | cold | 151.1 MiB | 24.71-24.71 s (21.2-21.2 MiB/s) | 11.00-11.00 s (47.7-47.7 MiB/s) | 0.026-0.026 s | 0 B | 41 % / 17.0 GiB | 80 % / 17.0 GiB |
| zip | Yes | cold | 152.4 MiB | 25.08-25.08 s (20.9-20.9 MiB/s) | 12.54-12.54 s (41.9-41.9 MiB/s) | 0.043-0.043 s | 0 B | 41 % / 17.1 GiB | 78 % / 17.2 GiB |
| 7z | No | cold | 80.2 MiB | 68.95-68.95 s (7.6-7.6 MiB/s) | 6.89-6.89 s (76.3-76.3 MiB/s) | 2.546-2.546 s | 0 B | 77 % / 18.8 GiB | 72 % / 17.5 GiB |
| 7z | Yes | cold | 80.2 MiB | 70.84-70.84 s (7.4-7.4 MiB/s) | 6.13-6.13 s (85.7-85.7 MiB/s) | 2.118-2.118 s | 0 B | 76 % / 18.8 GiB | 62 % / 17.5 GiB |
| tar | No | cold | 562.7 MiB | 8.60-8.60 s (61.1-61.1 MiB/s) | 2.53-2.53 s (207.7-207.7 MiB/s) | 0.501-0.501 s | 0 B | 47 % / 17.3 GiB | 29 % / 17.4 GiB |
| tar | Yes | cold | 562.7 MiB | 23.18-23.18 s (22.7-22.7 MiB/s) | 19.75-19.75 s (26.6-26.6 MiB/s) | 15.155-15.155 s | 0 B | 11 % / 17.5 GiB | 7 % / 17.9 GiB |
| tar.gz | No | cold | 122.2 MiB | 15.78-15.78 s (33.3-33.3 MiB/s) | 3.28-3.28 s (160.1-160.1 MiB/s) | 1.789-1.789 s | 0 B | 26 % / 16.9 GiB | 22 % / 17.2 GiB |
| tar.gz | Yes | cold | 122.2 MiB | 18.79-18.79 s (27.9-27.9 MiB/s) | 6.14-6.14 s (85.5-85.5 MiB/s) | 5.201-5.201 s | 0 B | 30 % / 17.2 GiB | 20 % / 17.5 GiB |

"Extract 3 files" is the time to extract only `fs/btrfs/check-integrity.c`,
`lib/xz/xz_dec_test.c`, `tools/perf/util/build-id.h` from the archive (for
encrypted tar/tar.gz this includes decrypting the archive). "Temp space" is the
peak disk usage of the results directory and the runner's temporary directory
during that extraction minus the size of the extracted files, sampled every 5 ms
(the maximum over all runs). Temporary files outside these directories (e.g. in
the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 4 extraction run(s),
`zarlib` renamed 48 path(s) total (12 per run) due to case-folded collisions;
generated suffixes are normalized only for comparison. The results directory was
emptied again after each check. Symlink entries are excluded because `zarlib`
currently reports and skips unsupported input symlinks. ZIP and tar preserve
links; 7z preserves them in its archive but may skip unsafe relative links
during extraction, as noted above.

CPU utilisation and RAM consumption were sampled at 1-second intervals during
each compression and extraction operation using `psutil`. The table shows the
average CPU % and RAM GiB across all samples for each phase. Runtime values show
the minimum and maximum measured time for each phase. The charts plot full
extraction times and the times to extract the three selected files separately.
The time-series charts plot CPU and RAM on separate axes for compression and
extraction, with one line per archive format (solid = plain, dashed =
encrypted). The CPU model and operating system are collected from the host.
Storage details describe the volumes holding the source/archive files and
extraction output. The operating system may not expose physical media behind
virtual or layered storage; such cases are identified as unknown or
non-rotational/possibly virtual.

## Method

- **Cache state**: `hot` runs read every file of the source tree (and, before
  extraction, the archive) once before the measured operation. `cold` runs drop
  the operating system file cache before every measured operation: `sync` plus
  writing `3` to `/proc/sys/vm/drop_caches` on Linux (root required), or
  `RAMMap.exe -Et` on Windows (elevation required). Cold runs are skipped with
  a warning when the privileges are missing.
- The installed 7-Zip does not support the zstd codec (only forks such as
  7-Zip-zstd do); the `7z-zstd` benchmark is skipped.
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

- `zarlib`: `0.9.0`
- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.9.0`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
