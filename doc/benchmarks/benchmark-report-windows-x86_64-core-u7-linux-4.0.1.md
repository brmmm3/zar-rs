# linux-4.0.1 Archive Benchmark

Run date: 2026-11-20T08:55:01+01:00
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
| zarlib | No | hot | 115.2 MiB | 3.89-4.49 s (116.9-134.9 MiB/s) | 7.61-7.64 s (68.7-69.0 MiB/s) | 0.171-0.184 s | 0 B | 24 % / 0.20 GiB | 62 % / 0.35 GiB |
| zarlib | Yes | hot | 115.3 MiB | 4.04-4.55 s (115.3-130.1 MiB/s) | 7.93-8.14 s (64.5-66.2 MiB/s) | 0.251-0.280 s | 0 B | 19 % / 0.24 GiB | 61 % / 0.57 GiB |
| zip | No | hot | 154.0 MiB | 23.40-23.50 s (22.3-22.4 MiB/s) | 55.33-59.10 s (8.9-9.5 MiB/s) | 0.085-0.087 s | 0 B | 18 % / 0.06 GiB | 29 % / 0.02 GiB |
| zip | Yes | hot | 155.3 MiB | 25.11-25.76 s (20.4-20.9 MiB/s) | 58.99-59.45 s (8.8-8.9 MiB/s) | 0.083-0.088 s | 0 B | 17 % / 0.06 GiB | 26 % / 0.02 GiB |
| 7z | No | hot | 80.3 MiB | 43.61-46.84 s (11.2-12.0 MiB/s) | 42.92-43.82 s (12.0-12.2 MiB/s) | 0.766-0.775 s | 0 B | 31 % / 1.76 GiB | 30 % / 0.57 GiB |
| 7z | Yes | hot | 80.3 MiB | 45.14-46.13 s (11.4-11.6 MiB/s) | 42.75-44.13 s (11.9-12.3 MiB/s) | 0.784-0.828 s | 0 B | 32 % / 1.74 GiB | 30 % / 0.57 GiB |
| tar | No | hot | 562.7 MiB | 3.10-4.02 s (130.8-169.2 MiB/s) | 24.78-26.77 s (19.6-21.2 MiB/s) | 0.178-0.187 s | 0 B | 17 % / 0.01 GiB | 33 % / 0.01 GiB |
| tar | Yes | hot | 562.7 MiB | 5.04-5.18 s (101.5-104.1 MiB/s) | 28.71-29.50 s (17.8-18.3 MiB/s) | 2.752-3.494 s | 0 B | 15 % / 0.23 GiB | 34 % / 0.07 GiB |
| tar.gz | No | hot | 119.4 MiB | 12.91-13.70 s (38.3-40.7 MiB/s) | 26.79-27.06 s (19.4-19.6 MiB/s) | 0.710-0.739 s | 0 B | 18 % / 0.01 GiB | 37 % / 0.01 GiB |
| tar.gz | Yes | hot | 119.4 MiB | 13.59-13.92 s (37.7-38.6 MiB/s) | 26.38-27.14 s (19.3-19.9 MiB/s) | 1.330-1.410 s | 0 B | 16 % / 0.01 GiB | 36 % / 0.01 GiB |
| zarlib | No | cold | 115.2 MiB | 6.17-6.17 s (85.1-85.1 MiB/s) | 7.73-7.73 s (68.0-68.0 MiB/s) | 0.181-0.181 s | 0 B | 28 % / 0.11 GiB | 65 % / 0.36 GiB |
| zarlib | Yes | cold | 115.1 MiB | 7.02-7.02 s (74.8-74.8 MiB/s) | 8.76-8.76 s (60.0-60.0 MiB/s) | 0.307-0.307 s | 0 B | 31 % / 0.17 GiB | 53 % / 0.63 GiB |
| zip | No | cold | 154.0 MiB | 46.35-46.35 s (11.3-11.3 MiB/s) | 57.82-57.82 s (9.1-9.1 MiB/s) | 0.100-0.100 s | 0 B | 19 % / 0.06 GiB | 25 % / 0.02 GiB |
| zip | Yes | cold | 155.3 MiB | 45.78-45.78 s (11.5-11.5 MiB/s) | 60.19-60.19 s (8.7-8.7 MiB/s) | 0.102-0.102 s | 0 B | 15 % / 0.06 GiB | 26 % / 0.02 GiB |
| 7z | No | cold | 80.3 MiB | 62.00-62.00 s (8.5-8.5 MiB/s) | 43.97-43.97 s (11.9-11.9 MiB/s) | 0.784-0.784 s | 0 B | 29 % / 1.43 GiB | 28 % / 0.57 GiB |
| 7z | Yes | cold | 80.3 MiB | 60.07-60.07 s (8.7-8.7 MiB/s) | 42.08-42.08 s (12.5-12.5 MiB/s) | 0.826-0.826 s | 0 B | 29 % / 1.45 GiB | 28 % / 0.58 GiB |
| tar | No | cold | 562.7 MiB | 21.33-21.33 s (24.6-24.6 MiB/s) | 27.56-27.56 s (19.1-19.1 MiB/s) | 0.881-0.881 s | 0 B | 13 % / 0.01 GiB | 34 % / 0.01 GiB |
| tar | Yes | cold | 562.7 MiB | 22.19-22.19 s (23.7-23.7 MiB/s) | 29.35-29.35 s (17.9-17.9 MiB/s) | 3.015-3.015 s | 0 B | 14 % / 0.05 GiB | 34 % / 0.09 GiB |
| tar.gz | No | cold | 119.4 MiB | 30.79-30.79 s (17.1-17.1 MiB/s) | 28.72-28.72 s (18.3-18.3 MiB/s) | 0.743-0.743 s | 0 B | 15 % / 0.01 GiB | 38 % / 0.01 GiB |
| tar.gz | Yes | cold | 119.4 MiB | 30.71-30.71 s (17.1-17.1 MiB/s) | 28.56-28.56 s (18.4-18.4 MiB/s) | 1.393-1.393 s | 0 B | 15 % / 0.01 GiB | 38 % / 0.01 GiB |



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

- `zarlib`: `0.15.0`
- `cargo`: `cargo 1.101.0-nightly (3c747ea3e 2026-11-03)`
- `tar`: `bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.13.1-motley liblzma/5.8.3 bz2lib/1.0.8 libzstd/1.5.7 cng/2.0 libb2/bundled`
- `openssl`: `OpenSSL 4.1.0 23 Oct 2026 (Library: OpenSSL 4.1.0 23 Oct 2026)`
- `7z`: `7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-09-03`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
