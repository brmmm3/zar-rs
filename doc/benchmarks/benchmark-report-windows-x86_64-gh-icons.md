# icons Archive Benchmark

Run date: 2026-11-20T17:42:53+00:00
- Operating system: Windows-2025Server-10.0.26100-SP0
- CPU: AMD EPYC 9V74 80-Core Processor (AMD64, 4 logical cores)
- Benchmark storage (Source and archives): Unknown (Windows reported no media type); model: Msft Virtual Disk; device: disk 1 (SAS)
- Benchmark storage (Extraction output): Unknown (Windows reported no media type); model: Msft Virtual Disk; device: disk 1 (SAS)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64-gh-icons.svg)

## Results

Source: [`icons.zar`](../../testdata/icons.zar)
Dataset: icon collection from a ZAR archive
Uncompressed source size: 29.5 MiB
Regular files verified: 7,958
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 23.5 MiB | 0.72-0.77 s (38.3-40.8 MiB/s) | 1.05-1.30 s (22.7-28.2 MiB/s) | 0.051-0.126 s | 0 B | 0 % / 0.03 GiB | 61 % / 0.08 GiB |
| zarlib | Yes | hot | 23.5 MiB | 0.84-0.85 s (34.7-35.2 MiB/s) | 1.18-1.22 s (24.2-24.9 MiB/s) | 0.165-0.176 s | 0 B | 0 % / 0.08 GiB | 36 % / 0.12 GiB |
| 7z | No | hot | 21.9 MiB | 2.27-2.42 s (12.2-13.0 MiB/s) | 2.96-3.09 s (9.5-10.0 MiB/s) | 0.285-0.288 s | 0 B | 38 % / 0.15 GiB | 21 % / 0.07 GiB |
| 7z | Yes | hot | 21.9 MiB | 2.29-2.29 s (12.9-12.9 MiB/s) | 3.10-3.11 s (9.5-9.5 MiB/s) | 0.325-0.498 s | 0 B | 38 % / 0.15 GiB | 23 % / 0.07 GiB |
| tar | No | hot | 35.4 MiB | 0.37-0.40 s (74.1-80.5 MiB/s) | 1.57-1.66 s (17.7-18.8 MiB/s) | 0.038-0.038 s | 0 B | N/A | 17 % / 0.01 GiB |
| tar | Yes | hot | 35.4 MiB | 0.55-0.65 s (45.6-54.0 MiB/s) | 1.78-1.95 s (15.1-16.6 MiB/s) | 0.251-0.329 s | 0 B | 0 % / 0.03 GiB | 16 % / 0.01 GiB |
| tar.gz | No | hot | 26.3 MiB | 1.34-1.34 s (22.0-22.0 MiB/s) | 1.62-1.74 s (16.9-18.2 MiB/s) | 0.129-0.130 s | 0 B | 14 % / 0.01 GiB | 16 % / 0.01 GiB |
| tar.gz | Yes | hot | 26.3 MiB | 1.51-1.52 s (19.4-19.5 MiB/s) | 2.09-2.14 s (13.8-14.1 MiB/s) | 0.325-0.383 s | 0 B | 24 % / 0.01 GiB | 30 % / 0.01 GiB |



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
- `cargo`: `cargo 1.100.0 (3acc79c0c 2026-11-05)`
- `tar`: `bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.13.1-motley liblzma/5.8.1 bz2lib/1.0.8 libzstd/1.5.7 cng/2.0 libb2/bundled`
- `openssl`: `OpenSSL 3.6.5 29 Sep 2026 (Library: OpenSSL 3.6.5 29 Sep 2026)`
- `7z`: `7-Zip 26.06 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-11-06`
- `gzip`: `gzip 1.15`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
