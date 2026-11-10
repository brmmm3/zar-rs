# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-08T23:07:18+01:00

- Operating system: Linux-5.15.0-198-generic-x86_64-with-glibc2.35
- CPU: Intel(R) Core(TM) i7-10700 CPU @ 2.90GHz (x86_64, 16 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be
  virtual); model: Not reported; device: dm-2
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual);
  model: Not reported; device: dm-2

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-core-i7.svg)

## Results

Source:
[`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Uncompressed source size: 525.1 MiB Regular files verified: 48,945 Symbolic
links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 115.5 MiB | 0.86-0.86 s (610.4-611.1 MiB/s) | 0.88-1.10 s (478.4-597.3 MiB/s) | 0.124-0.129 s | 0 B | 0 % / 0.0 GiB | 0 % / 7.0 GiB |
| zarlib | Yes | hot | 115.4 MiB | 0.96-1.00 s (524.3-545.5 MiB/s) | 1.21-1.22 s (430.2-432.8 MiB/s) | 0.345-0.354 s | 0 B | 12 % / 6.6 GiB | 19 % / 7.0 GiB |
| zip | No | hot | 151.1 MiB | 11.01-11.05 s (47.5-47.7 MiB/s) | 3.84-3.90 s (134.7-136.9 MiB/s) | 0.007-0.007 s | 0 B | 7 % / 6.5 GiB | 7 % / 6.5 GiB |
| zip | Yes | hot | 152.4 MiB | 11.52-11.59 s (45.3-45.6 MiB/s) | 4.96-4.97 s (105.7-105.8 MiB/s) | 0.012-0.012 s | 0 B | 7 % / 6.5 GiB | 8 % / 6.5 GiB |
| 7z | No | hot | 81.1 MiB | 24.91-24.94 s (21.1-21.1 MiB/s) | 5.68-5.70 s (92.1-92.5 MiB/s) | 3.407-3.434 s | 0 B | 65 % / 8.1 GiB | 7 % / 6.5 GiB |
| 7z | Yes | hot | 81.1 MiB | 24.80-24.93 s (21.1-21.2 MiB/s) | 5.83-5.86 s (89.6-90.1 MiB/s) | 3.553-3.583 s | 0 B | 65 % / 8.1 GiB | 7 % / 6.5 GiB |
| tar | No | hot | 562.7 MiB | 0.52-0.53 s (984.9-1014.8 MiB/s) | 0.91-1.00 s (525.8-576.0 MiB/s) | 0.063-0.065 s | 0 B | 0 % / 0.0 GiB | 0 % / 0.0 GiB |
| tar | Yes | hot | 562.7 MiB | 2.02-2.69 s (195.0-259.7 MiB/s) | 2.07-2.49 s (210.8-253.3 MiB/s) | 1.360-1.366 s | 0 B | 7 % / 7.2 GiB | 8 % / 6.8 GiB |
| tar.gz | No | hot | 122.1 MiB | 12.76-12.79 s (41.0-41.2 MiB/s) | 2.24-2.26 s (232.0-234.4 MiB/s) | 2.170-2.222 s | 0 B | 7 % / 6.5 GiB | 10 % / 6.5 GiB |
| tar.gz | Yes | hot | 122.1 MiB | 12.98-13.21 s (39.8-40.5 MiB/s) | 2.49-2.96 s (177.3-210.6 MiB/s) | 2.473-2.483 s | 0 B | 7 % / 6.5 GiB | 8 % / 6.5 GiB |
| zarlib | No | cold | 115.4 MiB | 1.03-1.03 s (508.7-508.7 MiB/s) | 1.13-1.13 s (465.9-465.9 MiB/s) | 0.161-0.161 s | 0 B | 10 % / 6.8 GiB | 22 % / 6.7 GiB |
| zarlib | Yes | cold | 115.4 MiB | 1.10-1.10 s (479.2-479.2 MiB/s) | 1.33-1.33 s (395.4-395.4 MiB/s) | 0.401-0.401 s | 0 B | 11 % / 6.8 GiB | 13 % / 6.6 GiB |
| zip | No | cold | 151.1 MiB | 17.47-17.47 s (30.1-30.1 MiB/s) | 4.13-4.13 s (127.1-127.1 MiB/s) | 0.020-0.020 s | 0 B | 5 % / 6.4 GiB | 8 % / 6.4 GiB |
| zip | Yes | cold | 152.4 MiB | 17.81-17.81 s (29.5-29.5 MiB/s) | 5.19-5.19 s (101.2-101.2 MiB/s) | 0.021-0.021 s | 0 B | 5 % / 6.4 GiB | 6 % / 6.4 GiB |
| 7z | No | cold | 81.1 MiB | 26.75-26.75 s (19.6-19.6 MiB/s) | 5.78-5.78 s (90.8-90.8 MiB/s) | 3.519-3.519 s | 0 B | 60 % / 7.9 GiB | 7 % / 6.4 GiB |
| 7z | Yes | cold | 81.1 MiB | 27.00-27.00 s (19.4-19.4 MiB/s) | 5.83-5.83 s (90.1-90.1 MiB/s) | 3.587-3.587 s | 0 B | 61 % / 7.9 GiB | 7 % / 6.4 GiB |
| tar | No | cold | 562.7 MiB | 7.05-7.05 s (74.4-74.4 MiB/s) | 1.10-1.10 s (477.4-477.4 MiB/s) | 0.870-0.870 s | 0 B | 3 % / 6.4 GiB | 5 % / 6.3 GiB |
| tar | Yes | cold | 562.7 MiB | 9.19-9.19 s (57.1-57.1 MiB/s) | 3.25-3.25 s (161.8-161.8 MiB/s) | 2.383-2.383 s | 0 B | 6 % / 6.6 GiB | 9 % / 6.8 GiB |
| tar.gz | No | cold | 122.1 MiB | 13.63-13.63 s (38.5-38.5 MiB/s) | 2.47-2.47 s (212.8-212.8 MiB/s) | 2.292-2.292 s | 0 B | 8 % / 6.4 GiB | 8 % / 6.4 GiB |
| tar.gz | Yes | cold | 122.1 MiB | 14.09-14.09 s (37.3-37.3 MiB/s) | 3.44-3.44 s (152.8-152.8 MiB/s) | 3.187-3.187 s | 0 B | 7 % / 6.4 GiB | 11 % / 6.4 GiB |

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
- `tar`: `tar (GNU tar) 1.34`
- `openssl`: `OpenSSL 3.0.2 15 Mar 2022 (Library: OpenSSL 3.0.2 15 Mar 2022)`
- `7z`: `7-Zip [64] 16.02 : Copyright (c) 1999-2016 Igor Pavlov : 2016-05-21`
- `gzip`: `gzip 1.10`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.9.0`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
