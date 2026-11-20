# icons Archive Benchmark

Run date: 2026-11-20T17:37:32+00:00
- Operating system: Linux-7.0.0-1020-azure-x86_64-with-glibc2.43
- CPU: AMD EPYC 7763 64-Core Processor (x86_64, 4 logical cores)
- Benchmark storage (Source and archives): HDD / rotational; model: Virtual Disk; device: sda
- Benchmark storage (Extraction output): HDD / rotational; model: Virtual Disk; device: sda

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-gh-icons.svg)

## Results

Source: [`icons.zar`](../../testdata/icons.zar)
Dataset: icon collection from a ZAR archive
Uncompressed source size: 29.5 MiB
Regular files verified: 7,958
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.5 MiB | 0.11-0.11 s (265.0-275.3 MiB/s) | 0.23-0.28 s (103.9-126.4 MiB/s) | 0.024-0.027 s | 0 B | N/A | N/A |
| zarlib | Yes | hot | 23.5 MiB | 0.20-0.21 s (142.1-145.1 MiB/s) | 0.35-0.36 s (81.5-84.5 MiB/s) | 0.125-0.127 s | 0 B | N/A | N/A |
| zip | No | hot | 30.9 MiB | 0.92-0.92 s (32.0-32.0 MiB/s) | 0.55-0.55 s (53.2-53.2 MiB/s) | 0.003-0.003 s | 0 B | 0 % / 0.01 GiB | 26 % / 0.00 GiB |
| zip | Yes | hot | 31.1 MiB | 1.05-1.06 s (27.9-28.1 MiB/s) | 0.92-0.93 s (31.8-32.2 MiB/s) | 0.008-0.009 s | 0 B | 19 % / 0.01 GiB | 26 % / 0.00 GiB |
| 7z | No | hot | 21.9 MiB | 2.05-2.16 s (13.7-14.4 MiB/s) | 0.96-1.00 s (29.5-30.6 MiB/s) | 0.214-0.216 s | 0 B | 34 % / 0.15 GiB | 28 % / 0.05 GiB |
| 7z | Yes | hot | 21.9 MiB | 2.10-2.36 s (12.5-14.0 MiB/s) | 0.99-0.99 s (29.7-29.8 MiB/s) | 0.239-0.241 s | 0 B | 38 % / 0.15 GiB | 27 % / 0.05 GiB |
| tar | No | hot | 35.4 MiB | 0.09-0.09 s (314.2-316.7 MiB/s) | 0.26-0.26 s (112.9-113.4 MiB/s) | 0.011-0.012 s | 0 B | N/A | N/A |
| tar | Yes | hot | 35.4 MiB | 0.27-0.28 s (104.4-110.5 MiB/s) | 0.40-0.41 s (71.5-74.3 MiB/s) | 0.164-0.168 s | 0 B | N/A | N/A |
| tar.gz | No | hot | 28.0 MiB | 0.81-0.81 s (36.6-36.6 MiB/s) | 0.28-0.29 s (103.4-105.2 MiB/s) | 0.124-0.126 s | 0 B | 15 % / 0.01 GiB | N/A |
| tar.gz | Yes | hot | 28.0 MiB | 0.94-0.95 s (31.2-31.3 MiB/s) | 0.41-0.41 s (71.9-72.0 MiB/s) | 0.252-0.264 s | 0 B | 26 % / 0.01 GiB | N/A |



"Extract 3 files" is the time to extract only `128x128/actions/address-book-new.png`, `128x128/places/folder.png`, `16x16/actions/address-book-new.png`
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
