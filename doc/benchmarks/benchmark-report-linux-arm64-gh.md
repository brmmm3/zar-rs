# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-08T21:46:17+00:00
- Operating system: Linux-7.0.0-1017-azure-aarch64-with-glibc2.39
- CPU: Not reported by operating system (aarch64, 2 logical cores)
- Benchmark storage (Source and archives): HDD / rotational; model: Virtual Disk; device: sda
- Benchmark storage (Extraction output): HDD / rotational; model: Virtual Disk; device: sda

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-arm64-gh.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 114.6 MiB | 1.87-1.88 s (279.4-281.2 MiB/s) | 2.00-2.35 s (223.4-262.9 MiB/s) | 0.144-0.149 s | 0 B | 0 % / 1.4 GiB | 68 % / 1.3 GiB |
| zarlib | Yes | hot | 114.6 MiB | 2.03-2.03 s (258.3-259.3 MiB/s) | 2.77-2.84 s (184.7-189.5 MiB/s) | 0.434-0.441 s | 0 B | 58 % / 1.5 GiB | 67 % / 1.4 GiB |
| zip | No | hot | 151.1 MiB | 11.35-11.38 s (46.2-46.3 MiB/s) | 4.46-4.49 s (116.9-117.8 MiB/s) | 0.009-0.010 s | 0 B | 50 % / 1.2 GiB | 48 % / 1.1 GiB |
| zip | Yes | hot | 152.4 MiB | 12.00-12.02 s (43.7-43.8 MiB/s) | 5.73-5.84 s (89.8-91.6 MiB/s) | 0.015-0.017 s | 0 B | 49 % / 1.1 GiB | 50 % / 1.0 GiB |
| 7z | No | hot | 80.7 MiB | 140.32-150.27 s (3.5-3.7 MiB/s) | 4.53-4.67 s (112.4-116.0 MiB/s) | 3.103-3.213 s | 0 B | 64 % / 1.2 GiB | 49 % / 1.0 GiB |
| 7z | Yes | hot | 80.7 MiB | 144.48-154.17 s (3.4-3.6 MiB/s) | 4.49-4.68 s (112.3-116.9 MiB/s) | 3.123-3.193 s | 0 B | 64 % / 1.2 GiB | 50 % / 1.0 GiB |
| tar | No | hot | 562.7 MiB | 0.59-0.61 s (857.7-894.1 MiB/s) | 1.34-1.41 s (372.9-390.7 MiB/s) | 0.079-0.080 s | 0 B | 0 % / 0.0 GiB | 48 % / 1.0 GiB |
| tar | Yes | hot | 562.7 MiB | 3.86-5.11 s (102.8-135.9 MiB/s) | 6.78-7.11 s (73.8-77.4 MiB/s) | 3.720-4.050 s | 0 B | 32 % / 1.6 GiB | 16 % / 1.4 GiB |
| tar.gz | No | hot | 122.2 MiB | 12.43-12.50 s (42.0-42.3 MiB/s) | 2.63-3.01 s (174.3-199.6 MiB/s) | 2.428-2.434 s | 0 B | 52 % / 1.1 GiB | 57 % / 1.2 GiB |
| tar.gz | Yes | hot | 122.2 MiB | 12.72-13.05 s (40.2-41.3 MiB/s) | 2.97-3.93 s (133.6-177.1 MiB/s) | 2.802-2.814 s | 0 B | 51 % / 1.2 GiB | 44 % / 1.3 GiB |

"Extract 3 files" is the time to extract only `fs/btrfs/check-integrity.c`, `lib/xz/xz_dec_test.c`, `tools/perf/util/build-id.h`
from the archive (for encrypted tar/tar.gz this includes decrypting the archive). "Temp space"
is the peak disk usage of the results directory and the runner's temporary
directory during that extraction minus the size of the extracted files, sampled
every 5 ms (the maximum over all runs). Temporary files outside these
directories (e.g. in the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 2 extraction run(s), `zarlib` renamed 24 path(s) total (12 per run) due to case-folded collisions; generated suffixes are normalized only for comparison. The results
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
- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.0.13 30 Jan 2024 (Library: OpenSSL 3.0.13 30 Jan 2024)`
- `7z`: `7-Zip 23.01 (arm64) : Copyright (c) 1999-2023 Igor Pavlov : 2023-06-20`
- `gzip`: `gzip 1.12`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.9.0`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
