# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-09T00:15:32+01:00
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
| zarlib | No | hot | 115.8 MiB | 2.28-2.40 s (218.5-230.5 MiB/s) | 33.84-36.11 s (14.5-15.5 MiB/s) | 0.228-0.242 s | 0 B | 18 % / 32.7 GiB | 86 % / 33.4 GiB |
| zip | No | hot | 154.6 MiB | 14.80-17.81 s (29.5-35.5 MiB/s) | 28.31-30.19 s (17.4-18.5 MiB/s) | 0.073-0.079 s | 0 B | 18 % / 33.4 GiB | 32 % / 33.5 GiB |
| zip | Yes | hot | 155.9 MiB | 17.59-17.83 s (29.4-29.8 MiB/s) | 29.22-29.42 s (17.8-18.0 MiB/s) | 0.074-0.130 s | 0 B | 18 % / 32.2 GiB | 30 % / 32.8 GiB |
| 7z | No | hot | 80.2 MiB | 42.90-66.33 s (7.9-12.2 MiB/s) | 339.76-367.20 s (1.4-1.5 MiB/s) | 0.793-0.817 s | 0 B | 44 % / 34.4 GiB | 14 % / 32.2 GiB |
| 7z | Yes | hot | 80.2 MiB | 44.52-48.59 s (10.8-11.8 MiB/s) | 330.02-349.91 s (1.5-1.6 MiB/s) | 0.783-0.832 s | 0 B | 28 % / 32.9 GiB | 13 % / 30.7 GiB |
| tar | No | hot | 562.6 MiB | 1.97-2.26 s (232.7-266.0 MiB/s) | 14.61-33.93 s (15.5-35.9 MiB/s) | 0.162-0.217 s | 0 B | 12 % / 29.9 GiB | 21 % / 29.4 GiB |
| tar | Yes | hot | 562.6 MiB | 5.63-8.70 s (60.4-93.2 MiB/s) | 11.13-11.40 s (46.1-47.2 MiB/s) | 2.967-2.995 s | 0 B | 12 % / 29.7 GiB | 33 % / 29.8 GiB |
| tar.gz | No | hot | 119.4 MiB | 11.46-15.52 s (33.8-45.8 MiB/s) | 9.05-9.39 s (55.9-58.0 MiB/s) | 0.750-0.756 s | 0 B | 11 % / 29.2 GiB | 35 % / 29.6 GiB |
| tar.gz | Yes | hot | 119.4 MiB | 19.31-19.54 s (26.9-27.2 MiB/s) | 9.27-10.75 s (48.8-56.7 MiB/s) | 1.336-1.375 s | 0 B | 12 % / 29.0 GiB | 30 % / 29.0 GiB |
| zarlib | No | cold | 115.2 MiB | 34.76-34.76 s (15.1-15.1 MiB/s) | 36.21-36.21 s (14.5-14.5 MiB/s) | 1.658-1.658 s | 0 B | 65 % / 27.7 GiB | 87 % / 27.9 GiB |
| zip | No | cold | 154.6 MiB | 368.49-368.49 s (1.4-1.4 MiB/s) | 22.41-22.41 s (23.4-23.4 MiB/s) | 0.147-0.147 s | 0 B | 10 % / 26.3 GiB | 27 % / 27.0 GiB |
| zip | Yes | cold | 155.9 MiB | 317.87-317.87 s (1.7-1.7 MiB/s) | 26.01-26.01 s (20.2-20.2 MiB/s) | 0.155-0.155 s | 0 B | 10 % / 26.4 GiB | 26 % / 27.1 GiB |
| 7z | No | cold | 80.2 MiB | 85.89-85.89 s (6.1-6.1 MiB/s) | 330.51-330.51 s (1.6-1.6 MiB/s) | 0.822-0.822 s | 0 B | 18 % / 27.4 GiB | 11 % / 27.1 GiB |
| 7z | Yes | cold | 80.2 MiB | 113.70-113.70 s (4.6-4.6 MiB/s) | 328.87-328.87 s (1.6-1.6 MiB/s) | 0.829-0.829 s | 0 B | 14 % / 27.3 GiB | 12 % / 27.1 GiB |
| tar | No | cold | 562.6 MiB | 356.08-356.08 s (1.5-1.5 MiB/s) | 10.36-10.36 s (50.7-50.7 MiB/s) | 1.078-1.078 s | 0 B | 11 % / 26.4 GiB | 37 % / 27.7 GiB |
| tar | Yes | cold | 562.6 MiB | 278.93-278.93 s (1.9-1.9 MiB/s) | 12.01-12.01 s (43.7-43.7 MiB/s) | 2.692-2.692 s | 0 B | 10 % / 26.7 GiB | 32 % / 27.4 GiB |
| tar.gz | No | cold | 119.4 MiB | 307.12-307.12 s (1.7-1.7 MiB/s) | 10.60-10.60 s (49.5-49.5 MiB/s) | 0.729-0.729 s | 0 B | 10 % / 26.6 GiB | 35 % / 27.3 GiB |
| tar.gz | Yes | cold | 119.4 MiB | 286.27-286.27 s (1.8-1.8 MiB/s) | 11.77-11.77 s (44.6-44.6 MiB/s) | 1.991-1.991 s | 0 B | 10 % / 26.5 GiB | 36 % / 27.2 GiB |

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

- `zarlib`: `0.9.0`
- `cargo`: `cargo 1.101.0-nightly (3c747ea3e 2026-11-03)`
- `tar`: `bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.13.1-motley liblzma/5.8.3 bz2lib/1.0.8 libzstd/1.5.7 cng/2.0 libb2/bundled`
- `openssl`: `OpenSSL 4.1.0 23 Oct 2026 (Library: OpenSSL 4.1.0 23 Oct 2026)`
- `7z`: `7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-09-03`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send`
- `zarc`: `zarc 0.9.0`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
