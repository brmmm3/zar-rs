# linux-6.1.1 Precompiled Kernel Archive Benchmark

Run date: 2026-11-20T19:03:10+00:00
- Operating system: Linux-7.0.0-1017-azure-aarch64-with-glibc2.39
- CPU: Not reported by operating system (aarch64, 4 logical cores)
- Benchmark storage (Source and archives): HDD / rotational; model: Virtual Disk; device: sda
- Benchmark storage (Extraction output): HDD / rotational; model: Virtual Disk; device: sda

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-arm64-gh-linux-6.1.1.svg)

## Results

Source: [`linux-6.1.1.zar`](../../testdata/linux-6.1.1.zar)
Dataset: precompiled Linux kernel tree from a ZAR archive
Uncompressed source size: 1445.3 MiB
Regular files verified: 88,384
Symbolic links excluded from path/size verification: 56

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 212.6 MiB | 1.94-1.96 s (735.6-743.7 MiB/s) | 2.85-5.23 s (276.1-507.8 MiB/s) | 0.173-0.176 s | 0 B | 50 % / 0.35 GiB | 34 % / 0.37 GiB |
| zarlib | Yes | hot | 212.3 MiB | 2.11-2.12 s (682.6-686.5 MiB/s) | 3.20-3.30 s (438.3-452.1 MiB/s) | 0.293-0.296 s | 0 B | 65 % / 0.35 GiB | 54 % / 0.42 GiB |
| zip | No | hot | 304.0 MiB | 25.93-25.98 s (55.6-55.7 MiB/s) | 9.55-9.57 s (151.0-151.4 MiB/s) | 0.015-0.016 s | 0 B | 25 % / 0.04 GiB | 26 % / 0.00 GiB |
| zip | Yes | hot | 306.3 MiB | 27.16-27.21 s (53.1-53.2 MiB/s) | 12.15-12.16 s (118.9-119.0 MiB/s) | 0.027-0.028 s | 0 B | 25 % / 0.04 GiB | 26 % / 0.00 GiB |
| 7z | No | hot | 146.1 MiB | 126.48-129.09 s (11.2-11.4 MiB/s) | 3.54-3.76 s (384.9-408.7 MiB/s) | 1.664-1.667 s | 0 B | 64 % / 0.51 GiB | 58 % / 0.27 GiB |
| 7z | Yes | hot | 146.1 MiB | 121.16-123.77 s (11.7-11.9 MiB/s) | 3.54-3.73 s (387.7-408.4 MiB/s) | 1.712-1.718 s | 0 B | 65 % / 0.51 GiB | 60 % / 0.27 GiB |
| tar | No | hot | 1512.4 MiB | 1.31-1.31 s (1100.4-1104.3 MiB/s) | 3.20-3.68 s (392.6-451.7 MiB/s) | 0.129-0.130 s | 0 B | 34 % / 0.00 GiB | 17 % / 0.00 GiB |
| tar | Yes | hot | 1512.4 MiB | 9.00-10.52 s (137.3-160.6 MiB/s) | 9.53-11.78 s (122.7-151.6 MiB/s) | 8.238-8.439 s | 0 B | 21 % / 1.22 GiB | 18 % / 0.60 GiB |
| tar.gz | No | hot | 262.1 MiB | 27.27-27.32 s (52.9-53.0 MiB/s) | 6.14-6.22 s (232.2-235.2 MiB/s) | 5.983-5.986 s | 0 B | 27 % / 0.01 GiB | 34 % / 0.00 GiB |
| tar.gz | Yes | hot | 262.1 MiB | 28.10-28.43 s (50.8-51.4 MiB/s) | 7.00-7.25 s (199.2-206.5 MiB/s) | 6.917-6.953 s | 0 B | 27 % / 0.02 GiB | 32 % / 0.04 GiB |



"Extract 3 files" is the time to extract only `Makefile`, `COPYING`, `kernel/sched/core.c`
from the archive (for encrypted tar/tar.gz this includes decrypting the archive). "Temp space"
is the peak disk usage of the extraction output and the runner's temporary
directory during that extraction minus the size of the extracted files, sampled
every 5 ms (the maximum over all runs). Temporary files outside these
directories (e.g. in the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/extracted/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 2 extraction run(s), `zarlib` renamed 0 path(s) total (0 per run) due to case-folded collisions; generated suffixes are normalized only for comparison. The results
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
- `cargo`: `cargo 1.100.0 (3acc79c0c 2026-11-05)`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.0.13 30 Jan 2024 (Library: OpenSSL 3.0.13 30 Jan 2024)`
- `7z`: `7-Zip 23.01 (arm64) : Copyright (c) 1999-2023 Igor Pavlov : 2023-06-20`
- `gzip`: `gzip 1.12`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
