# linux-6.1.1 Precompiled Kernel Archive Benchmark

Run date: 2026-11-26T17:31:35+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-core-u7-linux-6.1.1.svg)

## Results

Source: [`linux-6.1.1.zar`](../../testdata/linux-6.1.1.zar)
Dataset: precompiled Linux kernel tree from a ZAR archive
Uncompressed source size: 1445.3 MiB
Regular files verified: 88,384
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 213.5 MiB | 7.43-7.52 s (192.2-194.5 MiB/s) | 13.00-13.63 s (106.0-111.2 MiB/s) | 0.257-0.262 s | 0 B | 25 % / 0.23 GiB | 46 % / 0.55 GiB |
| zarlib | Yes | hot | 213.5 MiB | 7.53-7.57 s (191.0-192.0 MiB/s) | 13.86-13.95 s (103.6-104.3 MiB/s) | 0.334-0.379 s | 0 B | 22 % / 0.28 GiB | 46 % / 0.74 GiB |
| zip | No | hot | 310.5 MiB | 38.99-40.58 s (35.6-37.1 MiB/s) | 82.00-94.08 s (15.4-17.6 MiB/s) | 0.088-0.088 s | 0 B | 12 % / 0.10 GiB | 16 % / 0.02 GiB |
| zip | Yes | hot | 312.9 MiB | 40.28-41.29 s (35.0-35.9 MiB/s) | 81.09-82.78 s (17.5-17.8 MiB/s) | 0.089-0.095 s | 0 B | 11 % / 0.10 GiB | 15 % / 0.02 GiB |
| 7z | No | hot | 143.8 MiB | 51.31-51.61 s (28.0-28.2 MiB/s) | 64.09-64.31 s (22.5-22.6 MiB/s) | 0.956-1.036 s | 0 B | 49 % / 3.86 GiB | 17 % / 1.45 GiB |
| 7z | Yes | hot | 143.8 MiB | 51.01-51.68 s (28.0-28.3 MiB/s) | 64.82-67.00 s (21.6-22.3 MiB/s) | 0.940-0.946 s | 0 B | 49 % / 3.83 GiB | 18 % / 1.47 GiB |
| tar | No | hot | 1512.3 MiB | 6.15-6.37 s (227.0-234.9 MiB/s) | 31.36-31.92 s (45.3-46.1 MiB/s) | 0.305-0.313 s | 0 B | 12 % / 0.01 GiB | 21 % / 0.01 GiB |
| tar | Yes | hot | 1512.3 MiB | 12.29-12.43 s (116.3-117.6 MiB/s) | 39.16-39.77 s (36.3-36.9 MiB/s) | 8.479-8.480 s | 0 B | 12 % / 0.72 GiB | 20 % / 0.49 GiB |
| tar.gz | No | hot | 255.5 MiB | 25.46-26.34 s (54.9-56.8 MiB/s) | 33.88-39.65 s (36.5-42.7 MiB/s) | 1.437-1.451 s | 0 B | 12 % / 0.01 GiB | 24 % / 0.01 GiB |
| tar.gz | Yes | hot | 255.5 MiB | 25.64-25.78 s (56.1-56.4 MiB/s) | 32.29-32.59 s (44.4-44.8 MiB/s) | 2.706-2.798 s | 0 B | 10 % / 0.02 GiB | 21 % / 0.02 GiB |
| zarlib | No | cold | 212.8 MiB | 10.85-10.85 s (133.2-133.2 MiB/s) | 13.74-13.74 s (105.2-105.2 MiB/s) | 0.294-0.294 s | 0 B | 29 % / 0.17 GiB | 51 % / 0.56 GiB |
| zarlib | Yes | cold | 212.7 MiB | 11.68-11.68 s (123.7-123.7 MiB/s) | 14.22-14.22 s (101.7-101.7 MiB/s) | 0.392-0.392 s | 0 B | 31 % / 0.24 GiB | 49 % / 0.74 GiB |
| zip | No | cold | 310.5 MiB | 72.38-72.38 s (20.0-20.0 MiB/s) | 84.53-84.53 s (17.1-17.1 MiB/s) | 0.173-0.173 s | 0 B | 8 % / 0.10 GiB | 17 % / 0.02 GiB |
| zip | Yes | cold | 312.9 MiB | 74.68-74.68 s (19.4-19.4 MiB/s) | 84.64-84.64 s (17.1-17.1 MiB/s) | 0.164-0.164 s | 0 B | 9 % / 0.10 GiB | 14 % / 0.02 GiB |
| 7z | No | cold | 143.8 MiB | 71.43-71.43 s (20.2-20.2 MiB/s) | 68.01-68.01 s (21.3-21.3 MiB/s) | 0.960-0.960 s | 0 B | 34 % / 2.78 GiB | 17 % / 1.46 GiB |
| 7z | Yes | cold | 143.8 MiB | 70.98-70.98 s (20.4-20.4 MiB/s) | 66.66-66.66 s (21.7-21.7 MiB/s) | 0.979-0.979 s | 0 B | 34 % / 2.77 GiB | 16 % / 1.46 GiB |
| tar | No | cold | 1512.3 MiB | 36.38-36.38 s (39.7-39.7 MiB/s) | 33.53-33.53 s (43.1-43.1 MiB/s) | 1.936-1.936 s | 0 B | 7 % / 0.01 GiB | 20 % / 0.01 GiB |
| tar | Yes | cold | 1512.3 MiB | 42.45-42.45 s (34.0-34.0 MiB/s) | 42.45-42.45 s (34.0-34.0 MiB/s) | 8.612-8.612 s | 0 B | 8 % / 0.24 GiB | 19 % / 0.42 GiB |
| tar.gz | No | cold | 255.5 MiB | 55.38-55.38 s (26.1-26.1 MiB/s) | 34.55-34.55 s (41.8-41.8 MiB/s) | 1.472-1.472 s | 0 B | 8 % / 0.01 GiB | 20 % / 0.01 GiB |
| tar.gz | Yes | cold | 255.5 MiB | 56.68-56.68 s (25.5-25.5 MiB/s) | 36.02-36.02 s (40.1-40.1 MiB/s) | 2.803-2.803 s | 0 B | 10 % / 0.01 GiB | 20 % / 0.02 GiB |



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

- `zarlib`: `0.17.0`
- `cargo`: `cargo 1.101.0-nightly (3c747ea3e 2026-11-03)`
- `tar`: `bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.13.1-motley liblzma/5.8.3 bz2lib/1.0.8 libzstd/1.5.7 cng/2.0 libb2/bundled`
- `openssl`: `OpenSSL 4.1.0 23 Oct 2026 (Library: OpenSSL 4.1.0 23 Oct 2026)`
- `7z`: `7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-09-03`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send`
- `zarc`: `zarc 0.17.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
