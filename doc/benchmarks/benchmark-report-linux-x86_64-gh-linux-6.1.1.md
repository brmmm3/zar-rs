# linux-6.1.1 Precompiled Kernel Archive Benchmark

Run date: 2026-11-20T19:09:10+00:00
- Operating system: Linux-7.0.0-1020-azure-x86_64-with-glibc2.43
- CPU: INTEL(R) XEON(R) PLATINUM 8573C (x86_64, 4 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: MSFT NVMe Accelerator v1.0; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: MSFT NVMe Accelerator v1.0; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-gh-linux-6.1.1.svg)

## Results

Source: [`linux-6.1.1.zar`](../../testdata/linux-6.1.1.zar)
Dataset: precompiled Linux kernel tree from a ZAR archive
Uncompressed source size: 1445.3 MiB
Regular files verified: 88,384
Symbolic links excluded from path/size verification: 56

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 212.5 MiB | 2.72-2.76 s (524.6-530.7 MiB/s) | 3.28-4.43 s (326.2-441.2 MiB/s) | 0.219-0.224 s | 0 B | 65 % / 0.35 GiB | 47 % / 0.38 GiB |
| zarlib | Yes | hot | 212.5 MiB | 2.84-2.87 s (504.0-508.5 MiB/s) | 3.74-3.80 s (380.4-386.4 MiB/s) | 0.347-0.349 s | 0 B | 65 % / 0.38 GiB | 63 % / 0.38 GiB |
| zip | No | hot | 304.0 MiB | 36.16-36.21 s (39.9-40.0 MiB/s) | 11.61-11.79 s (122.6-124.5 MiB/s) | 0.016-0.016 s | 0 B | 25 % / 0.05 GiB | 26 % / 0.00 GiB |
| zip | Yes | hot | 306.3 MiB | 37.65-37.70 s (38.3-38.4 MiB/s) | 15.33-15.49 s (93.3-94.3 MiB/s) | 0.026-0.026 s | 0 B | 25 % / 0.05 GiB | 25 % / 0.00 GiB |
| 7z | No | hot | 143.9 MiB | 170.90-173.04 s (8.4-8.5 MiB/s) | 4.93-4.97 s (290.7-293.4 MiB/s) | 2.698-2.804 s | 0 B | 71 % / 0.94 GiB | 59 % / 0.50 GiB |
| 7z | Yes | hot | 143.9 MiB | 162.69-162.88 s (8.9-8.9 MiB/s) | 4.86-4.96 s (291.3-297.5 MiB/s) | 2.716-2.725 s | 0 B | 72 % / 0.95 GiB | 58 % / 0.50 GiB |
| tar | No | hot | 1512.4 MiB | 1.23-1.23 s (1172.5-1175.4 MiB/s) | 3.99-4.33 s (334.1-362.2 MiB/s) | 0.121-0.122 s | 0 B | 26 % / 0.00 GiB | 14 % / 0.00 GiB |
| tar | Yes | hot | 1512.4 MiB | 11.81-15.35 s (94.2-122.4 MiB/s) | 13.27-16.99 s (85.1-108.9 MiB/s) | 13.156-14.296 s | 0 B | 17 % / 1.29 GiB | 14 % / 0.66 GiB |
| tar.gz | No | hot | 262.1 MiB | 35.37-35.38 s (40.8-40.9 MiB/s) | 4.45-4.46 s (324.0-325.0 MiB/s) | 4.214-4.325 s | 0 B | 27 % / 0.01 GiB | 36 % / 0.00 GiB |
| tar.gz | Yes | hot | 262.1 MiB | 36.27-36.31 s (39.8-39.9 MiB/s) | 5.35-6.33 s (228.2-270.0 MiB/s) | 6.035-6.442 s | 0 B | 27 % / 0.01 GiB | 34 % / 0.07 GiB |



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
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
