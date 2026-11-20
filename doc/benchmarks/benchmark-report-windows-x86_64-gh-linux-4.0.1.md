# linux-4.0.1 Archive Benchmark

Run date: 2026-11-20T18:28:07+00:00
- Operating system: Windows-2025Server-10.0.26100-SP0
- CPU: AMD EPYC 9V45 96-Core Processor (AMD64, 4 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: Microsoft NVMe Direct Disk v2; device: disk 1 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: Microsoft NVMe Direct Disk v2; device: disk 1 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-gh-linux-4.0.1.svg)

## Results

Source: [`linux-4.0.1.zar`](../../testdata/linux-4.0.1.zar)
Dataset: Linux 4.0.1 source tree from a ZAR archive
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 114.6 MiB | 3.82-3.84 s (136.7-137.4 MiB/s) | 5.27-5.59 s (93.9-99.6 MiB/s) | 0.157-0.161 s | 0 B | 36 % / 0.11 GiB | 43 % / 0.22 GiB |
| zarlib | Yes | hot | 114.6 MiB | 3.94-4.09 s (128.3-133.2 MiB/s) | 5.36-5.48 s (95.8-98.0 MiB/s) | 0.263-0.278 s | 0 B | 46 % / 0.13 GiB | 43 % / 0.27 GiB |
| 7z | No | hot | 80.3 MiB | 71.09-80.99 s (6.5-7.4 MiB/s) | 11.62-12.57 s (41.8-45.2 MiB/s) | 0.995-1.082 s | 0 B | 70 % / 0.90 GiB | 27 % / 0.56 GiB |
| 7z | Yes | hot | 80.3 MiB | 69.68-71.79 s (7.3-7.5 MiB/s) | 12.06-12.33 s (42.6-43.5 MiB/s) | 1.098-1.169 s | 0 B | 65 % / 0.90 GiB | 30 % / 0.51 GiB |
| tar | No | hot | 562.7 MiB | 2.13-2.15 s (243.7-246.5 MiB/s) | 6.29-7.99 s (65.7-83.5 MiB/s) | 0.166-0.172 s | 0 B | 22 % / 0.01 GiB | 18 % / 0.01 GiB |
| tar | Yes | hot | 562.7 MiB | 5.14-6.30 s (83.3-102.2 MiB/s) | 10.67-11.52 s (45.6-49.2 MiB/s) | 1.863-1.900 s | 0 B | 14 % / 0.65 GiB | 17 % / 0.14 GiB |
| tar.gz | No | hot | 119.4 MiB | 13.08-13.19 s (39.8-40.1 MiB/s) | 6.00-7.67 s (68.5-87.4 MiB/s) | 0.708-0.721 s | 0 B | 27 % / 0.01 GiB | 20 % / 0.01 GiB |
| tar.gz | Yes | hot | 119.4 MiB | 12.98-13.08 s (40.1-40.4 MiB/s) | 6.44-7.30 s (72.0-81.5 MiB/s) | 1.071-1.112 s | 0 B | 26 % / 0.01 GiB | 22 % / 0.01 GiB |



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
- On this platform `zip`/`unzip` receive the password with `-P` instead of an interactive prompt.
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
- `tar`: `bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.13.1-motley liblzma/5.8.1 bz2lib/1.0.8 libzstd/1.5.7 cng/2.0 libb2/bundled`
- `openssl`: `OpenSSL 3.6.5 29 Sep 2026 (Library: OpenSSL 3.6.5 29 Sep 2026)`
- `7z`: `7-Zip 26.06 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-11-06`
- `gzip`: `gzip 1.15`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
