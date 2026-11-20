# duplicates Archive Benchmark

Run date: 2026-11-20T17:37:35+00:00
- Operating system: Linux-7.0.0-1020-azure-x86_64-with-glibc2.43
- CPU: AMD EPYC 9V74 80-Core Processor (x86_64, 4 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: MSFT NVMe Accelerator v1.0; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: MSFT NVMe Accelerator v1.0; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-gh-duplicates.svg)

## Results

Source: [`duplicates.zar`](../../testdata/duplicates.zar)
Dataset: duplicate icon tree from a ZAR archive
Uncompressed source size: 57.9 MiB
Regular files verified: 15,383
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.6 MiB | 0.17-0.17 s (338.6-343.5 MiB/s) | 0.37-0.45 s (128.6-156.4 MiB/s) | 0.030-0.031 s | 0 B | N/A | N/A |
| zarlib | Yes | hot | 23.6 MiB | 0.25-0.26 s (221.9-228.3 MiB/s) | 0.48-0.49 s (118.8-121.1 MiB/s) | 0.117-0.117 s | 0 B | N/A | N/A |
| zip | No | hot | 60.9 MiB | 1.55-1.55 s (37.3-37.4 MiB/s) | 0.90-0.90 s (64.1-64.1 MiB/s) | 0.004-0.004 s | 0 B | 13 % / 0.01 GiB | 26 % / 0.00 GiB |
| zip | Yes | hot | 61.3 MiB | 1.76-1.76 s (32.8-32.8 MiB/s) | 1.51-1.52 s (38.1-38.4 MiB/s) | 0.010-0.011 s | 0 B | 20 % / 0.01 GiB | 27 % / 0.00 GiB |
| 7z | No | hot | 22.0 MiB | 6.48-6.58 s (8.8-8.9 MiB/s) | 1.16-1.18 s (49.1-49.9 MiB/s) | 0.501-0.505 s | 0 B | 30 % / 0.27 GiB | 26 % / 0.08 GiB |
| 7z | Yes | hot | 22.0 MiB | 6.55-6.60 s (8.8-8.8 MiB/s) | 1.18-1.18 s (49.0-49.2 MiB/s) | 0.524-0.532 s | 0 B | 28 % / 0.28 GiB | 26 % / 0.07 GiB |
| tar | No | hot | 69.2 MiB | 0.17-0.17 s (340.8-345.6 MiB/s) | 0.40-0.40 s (144.5-146.5 MiB/s) | 0.018-0.019 s | 0 B | N/A | N/A |
| tar | Yes | hot | 69.2 MiB | 0.44-0.76 s (76.3-132.0 MiB/s) | 0.60-0.79 s (73.0-95.7 MiB/s) | 0.485-0.548 s | 0 B | 29 % / 0.02 GiB | 10 % / 0.00 GiB |
| tar.gz | No | hot | 55.0 MiB | 1.38-1.40 s (41.4-41.9 MiB/s) | 0.42-0.43 s (133.5-138.0 MiB/s) | 0.210-0.215 s | 0 B | 28 % / 0.01 GiB | N/A |
| tar.gz | Yes | hot | 55.0 MiB | 1.57-1.86 s (31.1-36.8 MiB/s) | 0.60-0.80 s (72.4-95.7 MiB/s) | 0.604-0.685 s | 0 B | 27 % / 0.01 GiB | 26 % / 0.00 GiB |



"Extract 3 files" is the time to extract only `duplicates/128x128/actions/address-book-new.png`, `duplicates/128x128/places/folder.png`, `icons/128x128/actions/address-book-new.png`
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
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
