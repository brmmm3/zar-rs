# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-01T16:54:22+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: Samsung SSD 990 EVO Plus 1TB; device: disk 1 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: Samsung SSD 990 EVO Plus 1TB; device: disk 1 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-core-u7.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Uncompressed source size: 525.0 MiB
Regular files verified: 48,933
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 151.3 MiB | 2.67-2.68 s (195.6-196.8 MiB/s) | 31.70-44.82 s (11.7-16.6 MiB/s) | 0.289-0.313 s | 0 B | 13 % / 24.3 GiB | 80 % / 25.2 GiB |
| zip | No | hot | 154.6 MiB | 15.96-18.29 s (28.7-32.9 MiB/s) | 31.65-38.13 s (13.8-16.6 MiB/s) | 0.067-0.072 s | 0 B | 17 % / 24.6 GiB | 26 % / 25.4 GiB |
| zip | Yes | hot | 155.9 MiB | 17.70-18.13 s (29.0-29.7 MiB/s) | 27.39-27.56 s (19.1-19.2 MiB/s) | 0.069-0.070 s | 0 B | 14 % / 24.4 GiB | 28 % / 25.6 GiB |
| 7z | No | hot | 80.2 MiB | 49.08-49.66 s (10.6-10.7 MiB/s) | 32.81-32.88 s (16.0-16.0 MiB/s) | 0.763-0.790 s | 0 B | 32 % / 26.2 GiB | 97 % / 26.2 GiB |
| 7z | Yes | hot | 80.2 MiB | 44.02-44.04 s (11.9-11.9 MiB/s) | 34.17-36.85 s (14.2-15.4 MiB/s) | 0.804-0.852 s | 0 B | 32 % / 26.2 GiB | 97 % / 26.4 GiB |
| tar | No | hot | 562.6 MiB | 2.16-2.42 s (217.2-243.6 MiB/s) | 17.08-18.88 s (27.8-30.7 MiB/s) | 0.167-0.182 s | 0 B | 7 % / 25.1 GiB | 29 % / 25.4 GiB |
| tar | Yes | hot | 562.6 MiB | 7.87-27.20 s (19.3-66.7 MiB/s) | 13.81-15.80 s (33.2-38.0 MiB/s) | 2.821-2.897 s | 0 B | 9 % / 26.5 GiB | 29 % / 26.4 GiB |
| tar.gz | No | hot | 119.4 MiB | 11.28-12.89 s (40.7-46.5 MiB/s) | 14.92-17.54 s (29.9-35.2 MiB/s) | 0.717-0.750 s | 0 B | 12 % / 25.7 GiB | 27 % / 25.6 GiB |
| tar.gz | Yes | hot | 119.4 MiB | 17.64-17.95 s (29.2-29.8 MiB/s) | 11.33-11.59 s (45.3-46.4 MiB/s) | 1.293-1.322 s | 0 B | 13 % / 24.9 GiB | 32 % / 25.6 GiB |
| zarlib | No | cold | 151.3 MiB | 33.88-33.88 s (15.5-15.5 MiB/s) | 30.29-30.29 s (17.3-17.3 MiB/s) | 0.449-0.449 s | 0 B | 58 % / 23.2 GiB | 94 % / 23.9 GiB |
| zip | No | cold | 154.6 MiB | 69.24-69.24 s (7.6-7.6 MiB/s) | 24.46-24.46 s (21.5-21.5 MiB/s) | 0.091-0.091 s | 0 B | 37 % / 23.0 GiB | 29 % / 23.7 GiB |
| zip | Yes | cold | 155.9 MiB | 45.35-45.35 s (11.6-11.6 MiB/s) | 25.48-25.48 s (20.6-20.6 MiB/s) | 0.170-0.170 s | 0 B | 47 % / 23.3 GiB | 27 % / 24.0 GiB |
| 7z | No | cold | 80.2 MiB | 67.41-67.41 s (7.8-7.8 MiB/s) | 31.53-31.53 s (16.7-16.7 MiB/s) | 0.799-0.799 s | 0 B | 23 % / 23.9 GiB | 96 % / 24.5 GiB |
| 7z | Yes | cold | 80.2 MiB | 72.33-72.33 s (7.3-7.3 MiB/s) | 32.57-32.57 s (16.1-16.1 MiB/s) | 0.829-0.829 s | 0 B | 23 % / 23.7 GiB | 96 % / 24.3 GiB |
| tar | No | cold | 562.6 MiB | 44.97-44.97 s (11.7-11.7 MiB/s) | 10.30-10.30 s (51.0-51.0 MiB/s) | 0.831-0.831 s | 0 B | 51 % / 23.3 GiB | 34 % / 24.1 GiB |
| tar | Yes | cold | 562.6 MiB | 41.67-41.67 s (12.6-12.6 MiB/s) | 13.00-13.00 s (40.4-40.4 MiB/s) | 3.005-3.005 s | 0 B | 50 % / 23.1 GiB | 30 % / 24.1 GiB |
| tar.gz | No | cold | 119.4 MiB | 42.57-42.57 s (12.3-12.3 MiB/s) | 11.42-11.42 s (46.0-46.0 MiB/s) | 0.759-0.759 s | 0 B | 48 % / 23.1 GiB | 33 % / 23.6 GiB |
| tar.gz | Yes | cold | 119.4 MiB | 42.27-42.27 s (12.4-12.4 MiB/s) | 11.86-11.86 s (44.3-44.3 MiB/s) | 1.368-1.368 s | 0 B | 50 % / 22.9 GiB | 30 % / 23.5 GiB |

"Extract 3 files" is the time to extract only `fs/btrfs/check-integrity.c`, `lib/xz/xz_dec_test.c`, `tools/perf/util/build-id.h`
from the archive (for encrypted tar/tar.gz this includes decrypting the archive). "Temp space"
is the peak disk usage of the results directory and the runner's temporary
directory during that extraction minus the size of the extracted files, sampled
every 5 ms (the maximum over all runs). Temporary files outside these
directories (e.g. in the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 2 extraction run(s), `zarlib` renamed 0 path(s) total (0 per run) due to case-folded collisions; generated suffixes are normalized only for comparison. The results
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
- `zarlib` encrypted runs are skipped on this platform because `zarc` can only prompt for passwords on POSIX terminals.
- On this platform `zip`/`unzip` receive the password with `-P` instead of an interactive prompt.
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

- `zarlib`: `0.8.0`
- `cargo`: `cargo 1.101.0-nightly (29c5daa1a 2026-10-07)`
- `tar`: `bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.13.1-motley liblzma/5.8.3 bz2lib/1.0.8 libzstd/1.5.7 cng/2.0 libb2/bundled`
- `openssl`: `OpenSSL 4.1.0 23 Oct 2026 (Library: OpenSSL 4.1.0 23 Oct 2026)`
- `7z`: `7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-09-03`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send`
- `zarc`: `zarc 0.8.0`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
