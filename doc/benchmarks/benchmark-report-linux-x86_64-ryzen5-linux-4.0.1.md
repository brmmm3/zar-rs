# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-18T10:34:54+01:00

- Operating system: Linux-7.2.15-070215-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-ryzen5-linux-4.0.1.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| zarlib | No | hot | 115.0 MiB | 2.61-2.80 s (187.3-201.0 MiB/s) | 2.78-2.96 s (177.2-189.1 MiB/s) | 0.200-0.205 s | 0 B | 64 % / 18.0 GiB | 81 % / 18.0 GiB |
| zarlib | Yes | hot | 114.9 MiB | 2.69-3.19 s (164.7-195.1 MiB/s) | 3.26-3.46 s (151.8-161.2 MiB/s) | 0.440-0.535 s | 0 B | 82 % / 18.3 GiB | 80 % / 18.3 GiB |
| zip | No | hot | 151.1 MiB | 17.48-20.45 s (25.7-30.0 MiB/s) | 8.15-9.12 s (57.6-64.4 MiB/s) | 0.013-0.024 s | 0 B | 42 % / 16.7 GiB | 78 % / 16.7 GiB |
| zip | Yes | hot | 152.4 MiB | 17.49-17.58 s (29.9-30.0 MiB/s) | 10.61-11.60 s (45.3-49.5 MiB/s) | 0.032-0.037 s | 0 B | 41 % / 15.1 GiB | 82 % / 15.2 GiB |
| 7z | No | hot | 80.2 MiB | 70.79-72.20 s (7.3-7.4 MiB/s) | 5.62-5.75 s (91.3-93.3 MiB/s) | 1.697-1.972 s | 0 B | 83 % / 16.8 GiB | 69 % / 15.7 GiB |
| 7z | Yes | hot | 80.2 MiB | 71.34-74.58 s (7.0-7.4 MiB/s) | 5.90-5.92 s (88.7-89.0 MiB/s) | 1.821-2.179 s | 0 B | 82 % / 16.5 GiB | 72 % / 15.4 GiB |
| tar | No | hot | 562.7 MiB | 1.57-1.59 s (330.6-335.1 MiB/s) | 2.43-2.51 s (209.4-215.7 MiB/s) | 0.108-0.114 s | 0 B | 47 % / 15.0 GiB | 42 % / 14.9 GiB |
| tar | Yes | hot | 562.7 MiB | 4.60-17.30 s (30.4-114.1 MiB/s) | 14.38-18.82 s (27.9-36.5 MiB/s) | 19.370-21.740 s | 0 B | 22 % / 15.5 GiB | 19 % / 15.3 GiB |
| tar.gz | No | hot | 122.2 MiB | 15.02-15.11 s (34.8-35.0 MiB/s) | 2.44-2.53 s (207.7-215.2 MiB/s) | 1.626-1.687 s | 0 B | 29 % / 15.1 GiB | 41 % / 15.2 GiB |
| tar.gz | Yes | hot | 122.2 MiB | 15.59-15.84 s (33.1-33.7 MiB/s) | 3.24-3.41 s (154.1-162.2 MiB/s) | 2.493-7.362 s | 0 B | 27 % / 15.0 GiB | 41 % / 15.5 GiB |
| zarlib | No | cold | 115.2 MiB | 3.11-3.11 s (168.8-168.8 MiB/s) | 3.05-3.05 s (171.9-171.9 MiB/s) | 0.269-0.269 s | 0 B | 74 % / 15.4 GiB | 65 % / 15.3 GiB |
| zarlib | Yes | cold | 115.2 MiB | 3.22-3.22 s (163.1-163.1 MiB/s) | 3.59-3.59 s (146.1-146.1 MiB/s) | 0.755-0.755 s | 0 B | 85 % / 15.4 GiB | 60 % / 15.4 GiB |
| zip | No | cold | 151.1 MiB | 23.99-23.99 s (21.9-21.9 MiB/s) | 9.35-9.35 s (56.2-56.2 MiB/s) | 0.021-0.021 s | 0 B | 40 % / 15.0 GiB | 68 % / 14.9 GiB |
| zip | Yes | cold | 152.4 MiB | 26.72-26.72 s (19.6-19.6 MiB/s) | 11.29-11.29 s (46.5-46.5 MiB/s) | 0.046-0.046 s | 0 B | 44 % / 15.2 GiB | 69 % / 15.1 GiB |
| 7z | No | cold | 80.2 MiB | 68.44-68.44 s (7.7-7.7 MiB/s) | 5.81-5.81 s (90.3-90.3 MiB/s) | 1.843-1.843 s | 0 B | 75 % / 16.5 GiB | 59 % / 15.6 GiB |
| 7z | Yes | cold | 80.2 MiB | 76.08-76.08 s (6.9-6.9 MiB/s) | 5.87-5.87 s (89.4-89.4 MiB/s) | 1.867-1.867 s | 0 B | 81 % / 16.5 GiB | 70 % / 15.4 GiB |
| tar | No | cold | 562.7 MiB | 7.61-7.61 s (69.0-69.0 MiB/s) | 2.52-2.52 s (208.1-208.1 MiB/s) | 0.520-0.520 s | 0 B | 53 % / 15.1 GiB | 38 % / 15.0 GiB |
| tar | Yes | cold | 562.7 MiB | 12.83-12.83 s (40.9-40.9 MiB/s) | 17.72-17.72 s (29.6-29.6 MiB/s) | 16.090-16.090 s | 0 B | 18 % / 15.1 GiB | 10 % / 15.3 GiB |
| tar.gz | No | cold | 122.2 MiB | 16.80-16.80 s (31.2-31.2 MiB/s) | 2.95-2.95 s (178.3-178.3 MiB/s) | 1.685-1.685 s | 0 B | 34 % / 15.1 GiB | 31 % / 15.0 GiB |
| tar.gz | Yes | cold | 122.2 MiB | 19.71-19.71 s (26.6-26.6 MiB/s) | 6.67-6.67 s (78.7-78.7 MiB/s) | 5.572-5.572 s | 0 B | 22 % / 14.9 GiB | 18 % / 15.0 GiB |

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

- `zarlib`: `0.14.0`
- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.14.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
