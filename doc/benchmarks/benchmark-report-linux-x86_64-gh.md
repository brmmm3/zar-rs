# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-08T21:50:39+00:00

- Operating system: Linux-7.0.0-1019-azure-x86_64-with-glibc2.43
- CPU: AMD EPYC 7763 64-Core Processor (x86_64, 2 logical cores)
- Benchmark storage (Source and archives): HDD / rotational; model: Virtual
  Disk; device: sda
- Benchmark storage (Extraction output): HDD / rotational; model: Virtual Disk;
  device: sda

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-gh.svg)

## Results

Source:
[`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Uncompressed source size: 525.1 MiB Regular files verified: 48,945 Symbolic
links excluded from path/size verification: 12

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 114.6 MiB | 2.77-2.95 s (177.8-189.3 MiB/s) | 3.08-3.52 s (149.2-170.2 MiB/s) | 0.179-0.181 s | 0 B | 50 % / 1.2 GiB | 86 % / 1.1 GiB |
| zarlib | Yes | hot | 114.6 MiB | 2.91-2.92 s (180.1-180.4 MiB/s) | 4.02-4.02 s (130.7-130.7 MiB/s) | 0.496-0.507 s | 0 B | 78 % / 1.3 GiB | 86 % / 1.2 GiB |
| zip | No | hot | 151.1 MiB | 15.23-15.28 s (34.4-34.5 MiB/s) | 6.40-6.53 s (80.3-82.0 MiB/s) | 0.010-0.010 s | 0 B | 49 % / 1.1 GiB | 53 % / 1.0 GiB |
| zip | Yes | hot | 152.4 MiB | 15.86-16.25 s (32.3-33.1 MiB/s) | 7.98-8.09 s (64.9-65.8 MiB/s) | 0.020-0.020 s | 0 B | 51 % / 1.1 GiB | 52 % / 1.0 GiB |
| 7z | No | hot | 79.9 MiB | 162.03-167.88 s (3.1-3.2 MiB/s) | 5.91-5.93 s (88.5-88.9 MiB/s) | 3.219-3.226 s | 0 B | 73 % / 1.4 GiB | 51 % / 1.1 GiB |
| 7z | Yes | hot | 79.9 MiB | 161.80-162.83 s (3.2-3.2 MiB/s) | 5.96-6.03 s (87.1-88.1 MiB/s) | 3.190-3.309 s | 0 B | 73 % / 1.4 GiB | 52 % / 1.1 GiB |
| tar | No | hot | 562.7 MiB | 0.90-1.14 s (458.8-580.9 MiB/s) | 2.45-2.45 s (214.3-214.5 MiB/s) | 0.076-0.078 s | 0 B | 70 % / 1.1 GiB | 54 % / 1.1 GiB |
| tar | Yes | hot | 562.7 MiB | 5.21-5.22 s (100.6-100.7 MiB/s) | 5.35-6.27 s (83.8-98.2 MiB/s) | 3.727-3.977 s | 0 B | 36 % / 1.6 GiB | 41 % / 1.4 GiB |
| tar.gz | No | hot | 122.1 MiB | 15.76-15.76 s (33.3-33.3 MiB/s) | 2.84-2.87 s (182.6-184.9 MiB/s) | 1.786-2.065 s | 0 B | 54 % / 1.2 GiB | 78 % / 1.1 GiB |
| tar.gz | Yes | hot | 122.1 MiB | 16.36-16.59 s (31.7-32.1 MiB/s) | 3.80-3.81 s (137.9-138.3 MiB/s) | 2.308-2.314 s | 0 B | 56 % / 1.2 GiB | 70 % / 1.3 GiB |

"Extract 3 files" is the time to extract only `fs/btrfs/check-integrity.c`,
`lib/xz/xz_dec_test.c`, `tools/perf/util/build-id.h` from the archive (for
encrypted tar/tar.gz this includes decrypting the archive). "Temp space" is the
peak disk usage of the results directory and the runner's temporary directory
during that extraction minus the size of the extracted files, sampled every 5 ms
(the maximum over all runs). Temporary files outside these directories (e.g. in
the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 2 extraction run(s),
`zarlib` renamed 24 path(s) total (12 per run) due to case-folded collisions;
generated suffixes are normalized only for comparison. The results directory was
emptied again after each check. Symlink entries are excluded because `zarlib`
currently reports and skips unsupported input symlinks. ZIP and tar preserve
links; 7z preserves them in its archive but may skip unsafe relative links
during extraction, as noted above.

CPU utilisation and RAM consumption were sampled at 1-second intervals during
each compression and extraction operation using `psutil`. The table shows the
average CPU % and RAM GiB across all samples for each phase. Runtime values show
the minimum and maximum measured time for each phase. The charts plot full
extraction times and the times to extract the three selected files separately.
The time-series charts plot CPU and RAM on separate axes for compression and
extraction, with one line per archive format (solid = plain, dashed =
encrypted). The CPU model and operating system are collected from the host.
Storage details describe the volumes holding the source/archive files and
extraction output. The operating system may not expose physical media behind
virtual or layered storage; such cases are identified as unknown or
non-rotational/possibly virtual.

## Method

- **Cache state**: `hot` runs read every file of the source tree (and, before
  extraction, the archive) once before the measured operation. `cold` runs drop
  the operating system file cache before every measured operation: `sync` plus
  writing `3` to `/proc/sys/vm/drop_caches` on Linux (root required), or
  `RAMMap.exe -Et` on Windows (elevation required). Cold runs are skipped with
  a warning when the privileges are missing.
- The installed 7-Zip does not support the zstd codec (only forks such as
  7-Zip-zstd do); the `7z-zstd` benchmark is skipped.
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
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.9.0`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
