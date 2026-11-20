# icons Archive Benchmark

Run date: 2026-11-20T07:13:07+01:00
- Operating system: Linux-5.15.0-198-generic-x86_64-with-glibc2.35
- CPU: Intel(R) Core(TM) i7-10700 CPU @ 2.90GHz (x86_64, 16 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-icons-core-i7.svg)

## Results

Source: [`icons.zar`](../../testdata/icons.zar)
Dataset: icon collection from a ZAR archive
Uncompressed source size: 29.5 MiB
Regular files verified: 7,958
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.5 MiB | 0.10-0.12 s (249.7-298.2 MiB/s) | 0.13-0.13 s (229.8-233.5 MiB/s) | 0.024-0.028 s | 0 B | N/A | N/A |
| zarlib | Yes | hot | 23.5 MiB | 0.18-0.18 s (160.9-162.1 MiB/s) | 0.21-0.23 s (130.8-138.7 MiB/s) | 0.116-0.132 s | 0 B | N/A | N/A |
| zip | No | hot | 30.9 MiB | 0.70-0.71 s (41.4-42.4 MiB/s) | 0.33-0.33 s (89.7-89.8 MiB/s) | 0.002-0.003 s | 0 B | 6 % / 0.01 GiB | N/A |
| zip | Yes | hot | 31.1 MiB | 0.80-0.82 s (36.0-37.0 MiB/s) | 0.57-0.61 s (48.4-51.7 MiB/s) | 0.006-0.006 s | 0 B | 12 % / 0.01 GiB | 16 % / 0.00 GiB |
| 7z | No | hot | 21.9 MiB | 2.40-2.51 s (11.7-12.3 MiB/s) | 1.54-1.55 s (19.1-19.2 MiB/s) | 0.329-0.343 s | 0 B | 18 % / 0.13 GiB | 16 % / 0.02 GiB |
| 7z | Yes | hot | 21.9 MiB | 2.57-2.58 s (11.5-11.5 MiB/s) | 1.65-1.66 s (17.8-17.9 MiB/s) | 0.460-0.461 s | 0 B | 16 % / 0.16 GiB | 10 % / 0.02 GiB |
| tar | No | hot | 35.4 MiB | 0.06-0.06 s (461.1-467.0 MiB/s) | 0.12-0.13 s (235.8-249.1 MiB/s) | 0.011-0.013 s | 0 B | N/A | N/A |
| tar | Yes | hot | 35.4 MiB | 0.19-0.21 s (142.5-151.8 MiB/s) | 0.22-0.22 s (131.9-136.3 MiB/s) | 0.120-0.123 s | 0 B | N/A | N/A |
| tar.gz | No | hot | 28.0 MiB | 0.74-0.75 s (39.4-39.6 MiB/s) | 0.19-0.22 s (133.2-154.2 MiB/s) | 0.172-0.175 s | 0 B | 10 % / 0.01 GiB | N/A |
| tar.gz | Yes | hot | 28.0 MiB | 0.82-0.87 s (33.7-36.0 MiB/s) | 0.27-0.28 s (106.6-108.5 MiB/s) | 0.263-0.278 s | 0 B | 8 % / 0.01 GiB | N/A |
| zarlib | No | cold | 23.5 MiB | 0.13-0.13 s (228.9-228.9 MiB/s) | 0.15-0.15 s (201.0-201.0 MiB/s) | 0.033-0.033 s | 0 B | N/A | N/A |
| zarlib | Yes | cold | 23.5 MiB | 0.22-0.22 s (133.6-133.6 MiB/s) | 0.24-0.24 s (123.4-123.4 MiB/s) | 0.137-0.137 s | 0 B | N/A | N/A |
| zip | No | cold | 30.9 MiB | 1.18-1.18 s (25.0-25.0 MiB/s) | 0.34-0.34 s (85.7-85.7 MiB/s) | 0.007-0.007 s | 0 B | 10 % / 0.01 GiB | N/A |
| zip | Yes | cold | 31.1 MiB | 1.27-1.27 s (23.2-23.2 MiB/s) | 0.61-0.61 s (48.3-48.3 MiB/s) | 0.012-0.012 s | 0 B | 9 % / 0.01 GiB | 13 % / 0.00 GiB |
| 7z | No | cold | 21.9 MiB | 2.58-2.58 s (11.4-11.4 MiB/s) | 1.57-1.57 s (18.8-18.8 MiB/s) | 0.340-0.340 s | 0 B | 21 % / 0.16 GiB | 11 % / 0.02 GiB |
| 7z | Yes | cold | 21.9 MiB | 2.82-2.82 s (10.5-10.5 MiB/s) | 1.71-1.71 s (17.2-17.2 MiB/s) | 0.496-0.496 s | 0 B | 19 % / 0.12 GiB | 16 % / 0.02 GiB |
| tar | No | cold | 35.4 MiB | 0.53-0.53 s (55.2-55.2 MiB/s) | 0.14-0.14 s (207.9-207.9 MiB/s) | 0.033-0.033 s | 0 B | 11 % / 0.00 GiB | N/A |
| tar | Yes | cold | 35.4 MiB | 0.71-0.71 s (41.8-41.8 MiB/s) | 0.27-0.27 s (110.8-110.8 MiB/s) | 0.125-0.125 s | 0 B | 23 % / 0.00 GiB | N/A |
| tar.gz | No | cold | 28.0 MiB | 0.93-0.93 s (31.6-31.6 MiB/s) | 0.23-0.23 s (126.9-126.9 MiB/s) | 0.196-0.196 s | 0 B | 26 % / 0.01 GiB | N/A |
| tar.gz | Yes | cold | 28.0 MiB | 0.97-0.97 s (30.3-30.3 MiB/s) | 0.32-0.32 s (91.3-91.3 MiB/s) | 0.282-0.282 s | 0 B | 16 % / 0.01 GiB | N/A |



"Extract 3 files" is the time to extract only `128x128/actions/address-book-new.png`, `128x128/places/folder.png`, `16x16/actions/address-book-new.png`
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
- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.34`
- `openssl`: `OpenSSL 3.0.2 15 Mar 2022 (Library: OpenSSL 3.0.2 15 Mar 2022)`
- `7z`: `7-Zip [64] 16.02 : Copyright (c) 1999-2016 Igor Pavlov : 2016-05-21`
- `gzip`: `gzip 1.10`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
