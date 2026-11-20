# linux-4.0.1 Archive Benchmark

Run date: 2026-11-20T18:23:40+00:00
- Operating system: Linux-7.0.0-1019-azure-x86_64-with-glibc2.43
- CPU: AMD EPYC 9V45 96-Core Processor (x86_64, 4 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: MSFT NVMe Accelerator v1.0; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: MSFT NVMe Accelerator v1.0; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-gh-linux-4.0.1.svg)

## Results

Source: [`linux-4.0.1.zar`](../../testdata/linux-4.0.1.zar)
Dataset: Linux 4.0.1 source tree from a ZAR archive
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 114.6 MiB | 1.10-1.17 s (450.0-478.7 MiB/s) | 1.36-2.02 s (260.2-385.1 MiB/s) | 0.082-0.087 s | 0 B | 49 % / 0.28 GiB | 49 % / 0.24 GiB |
| zarlib | Yes | hot | 114.7 MiB | 1.21-1.21 s (433.7-433.9 MiB/s) | 1.63-1.82 s (288.1-322.2 MiB/s) | 0.189-0.191 s | 0 B | 59 % / 0.30 GiB | 63 % / 0.31 GiB |
| zip | No | hot | 150.3 MiB | 11.01-11.07 s (47.4-47.7 MiB/s) | 4.75-4.75 s (110.5-110.7 MiB/s) | 0.006-0.006 s | 0 B | 25 % / 0.03 GiB | 25 % / 0.00 GiB |
| zip | Yes | hot | 151.6 MiB | 11.32-11.68 s (45.0-46.4 MiB/s) | 6.12-6.20 s (84.7-85.8 MiB/s) | 0.016-0.017 s | 0 B | 26 % / 0.03 GiB | 26 % / 0.00 GiB |
| 7z | No | hot | 80.3 MiB | 84.98-87.16 s (6.0-6.2 MiB/s) | 3.07-3.12 s (168.4-170.8 MiB/s) | 1.054-1.139 s | 0 B | 64 % / 0.89 GiB | 43 % / 0.48 GiB |
| 7z | Yes | hot | 80.3 MiB | 84.51-86.67 s (6.1-6.2 MiB/s) | 3.01-3.05 s (172.1-174.3 MiB/s) | 1.066-1.079 s | 0 B | 64 % / 0.89 GiB | 42 % / 0.39 GiB |
| tar | No | hot | 562.7 MiB | 0.70-0.70 s (746.7-746.8 MiB/s) | 1.39-1.40 s (376.2-378.3 MiB/s) | 0.049-0.052 s | 0 B | 51 % / 0.00 GiB | 24 % / 0.00 GiB |
| tar | Yes | hot | 562.7 MiB | 2.42-5.50 s (95.4-216.8 MiB/s) | 2.93-5.65 s (93.0-179.3 MiB/s) | 4.248-5.209 s | 0 B | 22 % / 0.19 GiB | 16 % / 0.12 GiB |
| tar.gz | No | hot | 122.1 MiB | 11.00-11.00 s (47.7-47.7 MiB/s) | 1.66-1.73 s (303.7-316.3 MiB/s) | 1.264-1.288 s | 0 B | 27 % / 0.01 GiB | 37 % / 0.00 GiB |
| tar.gz | Yes | hot | 122.1 MiB | 11.15-11.28 s (46.6-47.1 MiB/s) | 2.07-2.36 s (222.5-254.0 MiB/s) | 2.028-2.039 s | 0 B | 27 % / 0.02 GiB | 33 % / 0.01 GiB |



"Extract 3 files" is the time to extract only `fs/btrfs/check-integrity.c`, `lib/xz/xz_dec_test.c`, `tools/perf/util/build-id.h`
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
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
