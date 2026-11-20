# linux-6.1.1 Precompiled Kernel Archive Benchmark

Run date: 2026-11-20T07:49:26+01:00
- Operating system: Linux-5.15.0-198-generic-x86_64-with-glibc2.35
- CPU: Intel(R) Core(TM) i7-10700 CPU @ 2.90GHz (x86_64, 16 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-linux-6.1.1-core-i7.svg)

## Results

Source: [`linux-6.1.1.zar`](../../testdata/linux-6.1.1.zar)
Dataset: precompiled Linux kernel tree from a ZAR archive
Uncompressed source size: 1445.3 MiB
Regular files verified: 88,384
Symbolic links excluded from path/size verification: 56

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 213.9 MiB | 1.87-1.92 s (753.5-773.2 MiB/s) | 1.69-1.73 s (834.1-856.6 MiB/s) | 0.179-0.210 s | 0 B | 50 % / 0.75 GiB | 48 % / 0.63 GiB |
| zarlib | Yes | hot | 213.6 MiB | 1.94-2.11 s (685.8-746.8 MiB/s) | 2.28-2.31 s (624.6-632.7 MiB/s) | 0.279-0.304 s | 0 B | 80 % / 0.79 GiB | 64 % / 0.81 GiB |
| zip | No | hot | 304.0 MiB | 26.53-26.89 s (53.7-54.5 MiB/s) | 9.40-9.53 s (151.7-153.8 MiB/s) | 0.014-0.017 s | 0 B | 12 % / 0.05 GiB | 13 % / 0.00 GiB |
| zip | Yes | hot | 306.3 MiB | 27.70-27.88 s (51.8-52.2 MiB/s) | 11.80-11.97 s (120.8-122.4 MiB/s) | 0.024-0.025 s | 0 B | 14 % / 0.05 GiB | 14 % / 0.00 GiB |
| 7z | No | hot | 146.0 MiB | 56.11-57.40 s (25.2-25.8 MiB/s) | 12.58-12.76 s (113.2-114.9 MiB/s) | 5.993-6.082 s | 0 B | 57 % / 1.64 GiB | 12 % / 0.04 GiB |
| 7z | Yes | hot | 146.0 MiB | 55.87-56.27 s (25.7-25.9 MiB/s) | 13.12-13.18 s (109.7-110.2 MiB/s) | 6.355-6.361 s | 0 B | 57 % / 1.68 GiB | 13 % / 0.04 GiB |
| tar | No | hot | 1512.4 MiB | 1.46-1.46 s (986.7-990.7 MiB/s) | 1.99-2.02 s (716.5-724.9 MiB/s) | 0.120-0.120 s | 0 B | 15 % / 0.00 GiB | 11 % / 0.00 GiB |
| tar | Yes | hot | 1512.4 MiB | 5.92-6.55 s (220.5-244.1 MiB/s) | 5.91-6.36 s (227.3-244.6 MiB/s) | 4.504-4.520 s | 0 B | 13 % / 1.32 GiB | 12 % / 0.84 GiB |
| tar.gz | No | hot | 262.2 MiB | 28.75-28.97 s (49.9-50.3 MiB/s) | 5.52-5.58 s (258.9-261.6 MiB/s) | 5.456-5.461 s | 0 B | 10 % / 0.01 GiB | 11 % / 0.00 GiB |
| tar.gz | Yes | hot | 262.2 MiB | 29.22-29.54 s (48.9-49.5 MiB/s) | 6.25-6.28 s (230.0-231.3 MiB/s) | 6.156-6.267 s | 0 B | 9 % / 0.02 GiB | 12 % / 0.04 GiB |
| zarlib | No | cold | 213.7 MiB | 1.98-1.98 s (731.6-731.6 MiB/s) | 1.74-1.74 s (829.5-829.5 MiB/s) | 0.230-0.230 s | 0 B | 53 % / 0.67 GiB | 39 % / 0.62 GiB |
| zarlib | Yes | cold | 213.4 MiB | 2.23-2.23 s (647.5-647.5 MiB/s) | 2.45-2.45 s (590.6-590.6 MiB/s) | 0.361-0.361 s | 0 B | 53 % / 0.67 GiB | 56 % / 0.74 GiB |
| zip | No | cold | 304.0 MiB | 32.33-32.33 s (44.7-44.7 MiB/s) | 9.56-9.56 s (151.2-151.2 MiB/s) | 0.022-0.022 s | 0 B | 10 % / 0.05 GiB | 15 % / 0.00 GiB |
| zip | Yes | cold | 306.3 MiB | 33.91-33.91 s (42.6-42.6 MiB/s) | 11.89-11.89 s (121.6-121.6 MiB/s) | 0.033-0.033 s | 0 B | 14 % / 0.05 GiB | 14 % / 0.00 GiB |
| 7z | No | cold | 146.0 MiB | 60.52-60.52 s (23.9-23.9 MiB/s) | 12.99-12.99 s (111.2-111.2 MiB/s) | 6.323-6.323 s | 0 B | 58 % / 1.62 GiB | 12 % / 0.04 GiB |
| 7z | Yes | cold | 146.0 MiB | 58.50-58.50 s (24.7-24.7 MiB/s) | 12.91-12.91 s (112.0-112.0 MiB/s) | 6.181-6.181 s | 0 B | 54 % / 1.60 GiB | 13 % / 0.04 GiB |
| tar | No | cold | 1512.4 MiB | 7.96-7.96 s (181.5-181.5 MiB/s) | 2.26-2.26 s (640.1-640.1 MiB/s) | 0.903-0.903 s | 0 B | 11 % / 0.00 GiB | 10 % / 0.00 GiB |
| tar | Yes | cold | 1512.4 MiB | 12.93-12.93 s (111.8-111.8 MiB/s) | 6.78-6.78 s (213.3-213.3 MiB/s) | 4.821-4.821 s | 0 B | 7 % / 0.61 GiB | 11 % / 0.81 GiB |
| tar.gz | No | cold | 262.2 MiB | 29.49-29.49 s (49.0-49.0 MiB/s) | 5.66-5.66 s (255.1-255.1 MiB/s) | 5.642-5.642 s | 0 B | 9 % / 0.01 GiB | 11 % / 0.00 GiB |
| tar.gz | Yes | cold | 262.2 MiB | 30.00-30.00 s (48.2-48.2 MiB/s) | 6.46-6.46 s (223.8-223.8 MiB/s) | 6.299-6.299 s | 0 B | 9 % / 0.02 GiB | 13 % / 0.04 GiB |



"Extract 3 files" is the time to extract only `Makefile`, `COPYING`, `kernel/sched/core.c`
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
