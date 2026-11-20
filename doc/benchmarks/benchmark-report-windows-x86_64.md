# Linux 4.0.1 Archive Benchmark

Run date: 2026-11-19T21:37:22+01:00
- Operating system: Windows-11-10.0.26200-SP0
- CPU: Intel(R) Core(TM) Ultra 7 265HX (AMD64, 20 logical cores)
- Benchmark storage (Source and archives): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)
- Benchmark storage (Extraction output): SSD (NVMe); model: SAMSUNG MZVL21T0HCLR-00BH1; device: disk 0 (NVMe)

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-windows-x86_64.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)
Dataset: source tree
Uncompressed source size: 525.0 MiB
Regular files verified: 48,933
Symbolic links excluded from path/size verification: 0

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 115.4 MiB | 3.76-3.84 s (136.7-139.5 MiB/s) | 6.74-7.34 s (71.5-77.9 MiB/s) | 0.154-0.161 s | 0 B | 13 % / 0.13 GiB | 45 % / 0.36 GiB |
| zip | No | hot | FAILED | - | - | - | - | - | - |
| zip | Yes | hot | FAILED | - | - | - | - | - | - |
| 7z | No | hot | 80.2 MiB | 44.87-45.90 s (11.4-11.7 MiB/s) | 31.47-31.76 s (16.5-16.7 MiB/s) | 0.776-0.777 s | 0 B | 32 % / 1.76 GiB | 19 % / 0.57 GiB |
| 7z | Yes | hot | 80.2 MiB | 42.08-44.41 s (11.8-12.5 MiB/s) | 30.54-31.29 s (16.8-17.2 MiB/s) | 0.764-0.782 s | 0 B | 28 % / 1.74 GiB | 19 % / 0.57 GiB |
| tar | No | hot | 562.6 MiB | 2.98-3.07 s (171.3-176.4 MiB/s) | 17.10-17.30 s (30.3-30.7 MiB/s) | 0.173-0.182 s | 0 B | 9 % / 0.01 GiB | 23 % / 0.01 GiB |
| tar | Yes | hot | 562.6 MiB | 4.96-5.03 s (104.4-105.8 MiB/s) | 22.09-23.35 s (22.5-23.8 MiB/s) | 2.771-2.844 s | 0 B | 11 % / 0.25 GiB | 26 % / 0.10 GiB |
| tar.gz | No | hot | 119.4 MiB | 12.72-15.77 s (33.3-41.3 MiB/s) | 19.87-22.98 s (22.9-26.4 MiB/s) | 0.744-0.806 s | 0 B | 21 % / 0.01 GiB | 33 % / 0.01 GiB |
| tar.gz | Yes | hot | 119.4 MiB | 13.03-13.23 s (39.7-40.3 MiB/s) | 18.60-19.25 s (27.3-28.2 MiB/s) | 1.303-1.352 s | 0 B | 12 % / 0.02 GiB | 25 % / 0.02 GiB |
| zarlib | No | cold | 115.0 MiB | 5.42-5.42 s (96.8-96.8 MiB/s) | 7.58-7.58 s (69.3-69.3 MiB/s) | 0.186-0.186 s | 0 B | 30 % / 0.12 GiB | 48 % / 0.36 GiB |
| zip | No | cold | FAILED | - | - | - | - | - | - |
| zip | Yes | cold | FAILED | - | - | - | - | - | - |
| 7z | No | cold | 80.2 MiB | 57.79-57.79 s (9.1-9.1 MiB/s) | 32.30-32.30 s (16.3-16.3 MiB/s) | 0.817-0.817 s | 0 B | 25 % / 1.45 GiB | 19 % / 0.57 GiB |
| 7z | Yes | cold | 80.2 MiB | 58.63-58.63 s (9.0-9.0 MiB/s) | 32.79-32.79 s (16.0-16.0 MiB/s) | 0.781-0.781 s | 0 B | 22 % / 1.45 GiB | 19 % / 0.57 GiB |
| tar | No | cold | 562.6 MiB | 20.68-20.68 s (25.4-25.4 MiB/s) | 23.08-23.08 s (22.8-22.8 MiB/s) | 0.921-0.921 s | 0 B | 8 % / 0.01 GiB | 36 % / 0.01 GiB |
| tar | Yes | cold | 562.6 MiB | 23.17-23.17 s (22.7-22.7 MiB/s) | 22.58-22.58 s (23.2-23.2 MiB/s) | 2.871-2.871 s | 0 B | 10 % / 0.09 GiB | 23 % / 0.12 GiB |
| tar.gz | No | cold | 119.4 MiB | 29.48-29.48 s (17.8-17.8 MiB/s) | 19.12-19.12 s (27.5-27.5 MiB/s) | 0.721-0.721 s | 0 B | 8 % / 0.01 GiB | 22 % / 0.01 GiB |
| tar.gz | Yes | cold | 119.4 MiB | 30.14-30.14 s (17.4-17.4 MiB/s) | 20.30-20.30 s (25.9-25.9 MiB/s) | 1.347-1.347 s | 0 B | 10 % / 0.01 GiB | 21 % / 0.02 GiB |

<details><summary>zip (encrypted=False, hot cache), run 1, compression: FAILED</summary>
<pre>Command failed with exit code 16: C:\ProgramData\chocolatey\bin\zip.EXE -q -r -6 -y C:\Workspace\Rust\zar-rs\testdata\benchmark-archives\linux-4.0.1-zip.zip linux-4.0.1

zip error: Invalid command arguments (short option &#x27;y&#x27; not supported)
</pre>
</details>
<details><summary>zip (encrypted=False, hot cache), run 2, compression: FAILED</summary>
<pre>Command failed with exit code 16: C:\ProgramData\chocolatey\bin\zip.EXE -q -r -6 -y C:\Workspace\Rust\zar-rs\testdata\benchmark-archives\linux-4.0.1-zip.zip linux-4.0.1

zip error: Invalid command arguments (short option &#x27;y&#x27; not supported)
</pre>
</details>
<details><summary>zip (encrypted=True, hot cache), run 1, compression: FAILED</summary>
<pre>Command failed with exit code 16: C:\ProgramData\chocolatey\bin\zip.EXE -q -r -6 -y -P [redacted] C:\Workspace\Rust\zar-rs\testdata\benchmark-archives\linux-4.0.1-zip-encrypted.zip linux-4.0.1

zip error: Invalid command arguments (short option &#x27;y&#x27; not supported)
</pre>
</details>
<details><summary>zip (encrypted=True, hot cache), run 2, compression: FAILED</summary>
<pre>Command failed with exit code 16: C:\ProgramData\chocolatey\bin\zip.EXE -q -r -6 -y -P [redacted] C:\Workspace\Rust\zar-rs\testdata\benchmark-archives\linux-4.0.1-zip-encrypted.zip linux-4.0.1

zip error: Invalid command arguments (short option &#x27;y&#x27; not supported)
</pre>
</details>
<details><summary>zip (encrypted=False, cold cache), run 1, compression: FAILED</summary>
<pre>Command failed with exit code 16: C:\ProgramData\chocolatey\bin\zip.EXE -q -r -6 -y C:\Workspace\Rust\zar-rs\testdata\benchmark-archives\linux-4.0.1-zip.zip linux-4.0.1

zip error: Invalid command arguments (short option &#x27;y&#x27; not supported)
</pre>
</details>
<details><summary>zip (encrypted=True, cold cache), run 1, compression: FAILED</summary>
<pre>Command failed with exit code 16: C:\ProgramData\chocolatey\bin\zip.EXE -q -r -6 -y -P [redacted] C:\Workspace\Rust\zar-rs\testdata\benchmark-archives\linux-4.0.1-zip-encrypted.zip linux-4.0.1

zip error: Invalid command arguments (short option &#x27;y&#x27; not supported)
</pre>
</details>

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
from verification. `zarlib` stores links but does not restore them by default;
ZIP and tar restore links, and 7z may skip unsafe relative links during
extraction, as noted above.

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
The source archive and extracted source trees remain in `testdata/`.
