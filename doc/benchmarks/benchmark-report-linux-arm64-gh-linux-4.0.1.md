# linux-4.0.1 Archive Benchmark

Run date: 2026-11-20T18:23:22+00:00
- Operating system: Linux-7.0.0-1017-azure-aarch64-with-glibc2.39
- CPU: Not reported by operating system (aarch64, 4 logical cores)
- Benchmark storage (Source and archives): HDD / rotational; model: Virtual Disk; device: sda
- Benchmark storage (Extraction output): HDD / rotational; model: Virtual Disk; device: sda

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-arm64-gh-linux-4.0.1.svg)

## Results

Source: [`linux-4.0.1.zar`](../../testdata/linux-4.0.1.zar)
Dataset: Linux 4.0.1 source tree from a ZAR archive
Uncompressed source size: 525.1 MiB
Regular files verified: 48,945
Symbolic links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 114.6 MiB | 1.01-1.03 s (511.9-520.6 MiB/s) | 1.20-2.12 s (247.5-436.2 MiB/s) | 0.098-0.098 s | 0 B | 43 % / 0.25 GiB | 42 % / 0.29 GiB |
| zarlib | Yes | hot | 114.6 MiB | 1.11-1.14 s (462.4-471.6 MiB/s) | 1.47-1.50 s (349.5-357.6 MiB/s) | 0.202-0.203 s | 0 B | 58 % / 0.26 GiB | 43 % / 0.28 GiB |
| zip | No | hot | 150.3 MiB | 11.29-11.30 s (46.5-46.5 MiB/s) | 4.18-4.21 s (124.8-125.7 MiB/s) | 0.009-0.009 s | 0 B | 25 % / 0.03 GiB | 25 % / 0.00 GiB |
| zip | Yes | hot | 151.6 MiB | 11.94-11.94 s (44.0-44.0 MiB/s) | 5.59-5.60 s (93.7-93.9 MiB/s) | 0.019-0.020 s | 0 B | 24 % / 0.02 GiB | 25 % / 0.00 GiB |
| 7z | No | hot | 81.2 MiB | 67.95-68.62 s (7.7-7.7 MiB/s) | 1.85-1.86 s (282.5-284.5 MiB/s) | 0.937-0.939 s | 0 B | 62 % / 0.47 GiB | 66 % / 0.30 GiB |
| 7z | Yes | hot | 81.2 MiB | 67.51-68.59 s (7.7-7.8 MiB/s) | 1.86-1.87 s (281.0-281.9 MiB/s) | 0.967-0.982 s | 0 B | 63 % / 0.47 GiB | 64 % / 0.30 GiB |
| tar | No | hot | 562.7 MiB | 0.57-0.58 s (906.3-913.4 MiB/s) | 0.94-1.03 s (510.9-558.9 MiB/s) | 0.070-0.072 s | 0 B | 57 % / 0.00 GiB | 21 % / 0.00 GiB |
| tar | Yes | hot | 562.7 MiB | 2.26-3.59 s (146.3-232.8 MiB/s) | 2.58-3.49 s (150.5-203.8 MiB/s) | 2.708-2.749 s | 0 B | 25 % / 0.36 GiB | 20 % / 0.20 GiB |
| tar.gz | No | hot | 122.1 MiB | 12.36-12.38 s (42.4-42.5 MiB/s) | 2.48-2.56 s (204.9-211.3 MiB/s) | 2.420-2.434 s | 0 B | 26 % / 0.01 GiB | 32 % / 0.00 GiB |
| tar.gz | Yes | hot | 122.1 MiB | 12.71-12.97 s (40.5-41.3 MiB/s) | 2.82-2.90 s (181.2-186.5 MiB/s) | 2.954-3.071 s | 0 B | 27 % / 0.01 GiB | 31 % / 0.00 GiB |



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
- `openssl`: `OpenSSL 3.0.13 30 Jan 2024 (Library: OpenSSL 3.0.13 30 Jan 2024)`
- `7z`: `7-Zip 23.01 (arm64) : Copyright (c) 1999-2023 Igor Pavlov : 2023-06-20`
- `gzip`: `gzip 1.12`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
