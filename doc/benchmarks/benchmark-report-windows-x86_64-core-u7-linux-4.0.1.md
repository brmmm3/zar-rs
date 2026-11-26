# linux-4.0.1 Archive Benchmark

Run date: 2026-11-26T16:32:59+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-core-u7-linux-4.0.1.svg)

## Results

Source: [`linux-4.0.1.zar`](../../testdata/linux-4.0.1.zar)
Dataset: Linux 4.0.1 source tree from a ZAR archive
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 115.2 MiB | 4.18-5.31 s (98.9-125.7 MiB/s) | 7.10-8.10 s (64.8-73.9 MiB/s) | 0.165-0.176 s | 0 B | 30 % / 0.16 GiB | 45 % / 0.35 GiB |
| zarlib | Yes | hot | 115.1 MiB | 4.24-4.33 s (121.2-123.9 MiB/s) | 7.52-7.71 s (68.1-69.8 MiB/s) | 0.250-0.261 s | 0 B | 22 % / 0.22 GiB | 46 % / 0.60 GiB |
| zip | No | hot | 154.0 MiB | 19.94-20.63 s (25.5-26.3 MiB/s) | 42.37-44.20 s (11.9-12.4 MiB/s) | 0.072-0.075 s | 0 B | 12 % / 0.06 GiB | 15 % / 0.02 GiB |
| zip | Yes | hot | 155.3 MiB | 20.25-20.39 s (25.8-25.9 MiB/s) | 44.04-44.19 s (11.9-11.9 MiB/s) | 0.073-0.075 s | 0 B | 9 % / 0.06 GiB | 16 % / 0.02 GiB |
| 7z | No | hot | 80.3 MiB | 42.11-44.87 s (11.7-12.5 MiB/s) | 35.26-38.73 s (13.6-14.9 MiB/s) | 0.758-0.788 s | 0 B | 28 % / 1.76 GiB | 18 % / 0.57 GiB |
| 7z | Yes | hot | 80.3 MiB | 42.25-43.33 s (12.1-12.4 MiB/s) | 36.61-39.41 s (13.3-14.3 MiB/s) | 0.750-0.805 s | 0 B | 27 % / 1.72 GiB | 21 % / 0.57 GiB |
| tar | No | hot | 562.7 MiB | 3.50-6.10 s (86.0-150.2 MiB/s) | 17.07-20.38 s (25.8-30.8 MiB/s) | 0.179-0.183 s | 0 B | 15 % / 0.01 GiB | 23 % / 0.01 GiB |
| tar | Yes | hot | 562.7 MiB | 5.37-5.49 s (95.7-97.8 MiB/s) | 19.44-22.29 s (23.6-27.0 MiB/s) | 2.922-3.041 s | 0 B | 13 % / 0.27 GiB | 22 % / 0.12 GiB |
| tar.gz | No | hot | 119.4 MiB | 12.23-12.83 s (40.9-42.9 MiB/s) | 16.95-17.03 s (30.8-31.0 MiB/s) | 0.730-0.741 s | 0 B | 13 % / 0.01 GiB | 21 % / 0.01 GiB |
| tar.gz | Yes | hot | 119.4 MiB | 12.74-13.15 s (39.9-41.2 MiB/s) | 18.19-18.63 s (28.2-28.9 MiB/s) | 1.372-1.392 s | 0 B | 11 % / 0.01 GiB | 22 % / 0.02 GiB |
| zarlib | No | cold | 115.2 MiB | 5.86-5.86 s (89.5-89.5 MiB/s) | 7.38-7.38 s (71.1-71.1 MiB/s) | 0.177-0.177 s | 0 B | 24 % / 0.11 GiB | 45 % / 0.37 GiB |
| zarlib | Yes | cold | 115.0 MiB | 5.97-5.97 s (87.9-87.9 MiB/s) | 7.61-7.61 s (69.0-69.0 MiB/s) | 0.262-0.262 s | 0 B | 21 % / 0.15 GiB | 39 % / 0.57 GiB |
| zip | No | cold | 154.0 MiB | 39.43-39.43 s (13.3-13.3 MiB/s) | 44.21-44.21 s (11.9-11.9 MiB/s) | 0.141-0.141 s | 0 B | 11 % / 0.06 GiB | 15 % / 0.02 GiB |
| zip | Yes | cold | 155.3 MiB | 39.17-39.17 s (13.4-13.4 MiB/s) | 44.94-44.94 s (11.7-11.7 MiB/s) | 0.137-0.137 s | 0 B | 10 % / 0.06 GiB | 14 % / 0.02 GiB |
| 7z | No | cold | 80.3 MiB | 55.92-55.92 s (9.4-9.4 MiB/s) | 36.03-36.03 s (14.6-14.6 MiB/s) | 0.786-0.786 s | 0 B | 23 % / 1.44 GiB | 17 % / 0.56 GiB |
| 7z | Yes | cold | 80.3 MiB | 57.44-57.44 s (9.1-9.1 MiB/s) | 37.22-37.22 s (14.1-14.1 MiB/s) | 0.794-0.794 s | 0 B | 22 % / 1.43 GiB | 18 % / 0.57 GiB |
| tar | No | cold | 562.7 MiB | 20.92-20.92 s (25.1-25.1 MiB/s) | 18.27-18.27 s (28.7-28.7 MiB/s) | 0.900-0.900 s | 0 B | 7 % / 0.01 GiB | 19 % / 0.01 GiB |
| tar | Yes | cold | 562.7 MiB | 22.03-22.03 s (23.8-23.8 MiB/s) | 21.36-21.36 s (24.6-24.6 MiB/s) | 2.959-2.959 s | 0 B | 8 % / 0.06 GiB | 19 % / 0.12 GiB |
| tar.gz | No | cold | 119.4 MiB | 29.43-29.43 s (17.8-17.8 MiB/s) | 18.81-18.81 s (27.9-27.9 MiB/s) | 0.740-0.740 s | 0 B | 8 % / 0.01 GiB | 19 % / 0.01 GiB |
| tar.gz | Yes | cold | 119.4 MiB | 30.24-30.24 s (17.4-17.4 MiB/s) | 19.21-19.21 s (27.3-27.3 MiB/s) | 1.487-1.487 s | 0 B | 8 % / 0.01 GiB | 19 % / 0.02 GiB |



"Extract 3 files" is the time to extract only `fs/btrfs/check-integrity.c`, `lib/xz/xz_dec_test.c`, `tools/perf/util/build-id.h`
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
