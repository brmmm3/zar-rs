# Linux 6.6.1 Compiled Kernel Benchmark

Run date: 2026-11-19T14:45:08+01:00
- Operating system: Linux-5.15.0-198-generic-x86_64-with-glibc2.35
- CPU: Intel(R) Core(TM) i7-10700 CPU @ 2.90GHz (x86_64, 16 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 990 PRO 1TB; device: nvme0n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-compiled-core-i7.svg)

## Results

Source: [`linux-6.6.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.6.1.tar.gz)
Dataset: source tree after `make defconfig` and `make`
Uncompressed source size: 1973.1 MiB
Regular files verified: 92,165
Symbolic links excluded from path/size verification: 64

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 376.3 MiB | 2.46-2.47 s (799.2-803.2 MiB/s) | 1.95-2.09 s (942.3-1012.5 MiB/s) | 0.191-0.225 s | 0 B | 65 % / 1.02 GiB | 42 % / 0.70 GiB |
| zarlib | Yes | hot | 376.1 MiB | 2.52-2.71 s (728.7-782.3 MiB/s) | 2.80-2.88 s (684.7-703.9 MiB/s) | 0.297-0.327 s | 0 B | 69 % / 0.97 GiB | 60 % / 0.82 GiB |
| zip | No | hot | 469.0 MiB | 39.39-39.61 s (49.8-50.1 MiB/s) | 12.04-12.40 s (159.1-163.9 MiB/s) | 0.014-0.015 s | 0 B | 11 % / 0.05 GiB | 15 % / 0.00 GiB |
| zip | Yes | hot | 471.5 MiB | 41.18-41.27 s (47.8-47.9 MiB/s) | 15.63-15.71 s (125.6-126.2 MiB/s) | 0.024-0.028 s | 0 B | 12 % / 0.05 GiB | 13 % / 0.00 GiB |
| 7z | No | hot | 251.4 MiB | 77.22-77.78 s (25.4-25.6 MiB/s) | 16.90-16.99 s (116.1-116.7 MiB/s) | 7.058-7.330 s | 0 B | 58 % / 1.59 GiB | 14 % / 0.04 GiB |
| 7z | Yes | hot | 251.4 MiB | 77.32-77.87 s (25.3-25.5 MiB/s) | 16.83-16.85 s (117.1-117.3 MiB/s) | 7.165-7.340 s | 0 B | 58 % / 1.59 GiB | 12 % / 0.04 GiB |
| tar | No | hot | 2043.1 MiB | 1.82-1.90 s (1036.8-1084.7 MiB/s) | 2.42-2.43 s (813.0-816.2 MiB/s) | 0.123-0.130 s | 0 B | 33 % / 0.00 GiB | 12 % / 0.00 GiB |
| tar | Yes | hot | FAILED | - | - | - | - | - | - |
| tar.gz | No | hot | 424.9 MiB | 40.93-41.05 s (48.1-48.2 MiB/s) | 7.72-7.74 s (254.8-255.6 MiB/s) | 7.661-7.708 s | 0 B | 10 % / 0.01 GiB | 11 % / 0.00 GiB |
| tar.gz | Yes | hot | 424.9 MiB | 42.60-42.82 s (46.1-46.3 MiB/s) | 8.74-8.95 s (220.4-225.7 MiB/s) | 8.848-8.915 s | 0 B | 10 % / 0.03 GiB | 12 % / 0.09 GiB |
| zarlib | No | cold | 376.6 MiB | 2.66-2.66 s (743.2-743.2 MiB/s) | 2.22-2.22 s (887.9-887.9 MiB/s) | 0.242-0.242 s | 0 B | 68 % / 0.91 GiB | 56 % / 0.64 GiB |
| zarlib | Yes | cold | 376.4 MiB | 2.79-2.79 s (707.5-707.5 MiB/s) | 2.93-2.93 s (674.3-674.3 MiB/s) | 0.351-0.351 s | 0 B | 67 % / 0.92 GiB | 56 % / 0.82 GiB |
| zip | No | cold | 469.0 MiB | 45.28-45.28 s (43.6-43.6 MiB/s) | 12.24-12.24 s (161.1-161.1 MiB/s) | 0.028-0.028 s | 0 B | 9 % / 0.05 GiB | 14 % / 0.00 GiB |
| zip | Yes | cold | 471.5 MiB | 47.47-47.47 s (41.6-41.6 MiB/s) | 15.75-15.75 s (125.3-125.3 MiB/s) | 0.035-0.035 s | 0 B | 12 % / 0.05 GiB | 13 % / 0.00 GiB |
| 7z | No | cold | 251.4 MiB | 81.36-81.36 s (24.3-24.3 MiB/s) | 17.08-17.08 s (115.5-115.5 MiB/s) | 7.002-7.002 s | 0 B | 59 % / 1.57 GiB | 14 % / 0.04 GiB |
| 7z | Yes | cold | 251.4 MiB | 78.77-78.77 s (25.0-25.0 MiB/s) | 17.08-17.08 s (115.5-115.5 MiB/s) | 7.338-7.338 s | 0 B | 57 % / 1.58 GiB | 14 % / 0.04 GiB |
| tar | No | cold | 2043.1 MiB | 8.68-8.68 s (227.4-227.4 MiB/s) | 2.71-2.71 s (728.4-728.4 MiB/s) | 1.132-1.132 s | 0 B | 8 % / 0.00 GiB | 9 % / 0.00 GiB |
| tar | Yes | cold | FAILED | - | - | - | - | - | - |
| tar.gz | No | cold | 424.9 MiB | 42.33-42.33 s (46.6-46.6 MiB/s) | 7.99-7.99 s (246.9-246.9 MiB/s) | 7.918-7.918 s | 0 B | 10 % / 0.01 GiB | 13 % / 0.00 GiB |
| tar.gz | Yes | cold | 424.9 MiB | 42.56-42.56 s (46.4-46.4 MiB/s) | 9.18-9.18 s (215.0-215.0 MiB/s) | 9.053-9.053 s | 0 B | 10 % / 0.03 GiB | 10 % / 0.07 GiB |

<details><summary>tar (encrypted=True, hot cache), run 1, compression: FAILED</summary>
<pre>Command failed with exit code 3: /usr/bin/openssl cms -encrypt -binary -aes-256-gcm -in /media/bam5wi/data/workspace/zar-rs/testdata/zar-benchmark-ysqx_ie5/encrypted-source.tar -out /media/bam5wi/data/workspace/zar-rs/testdata/benchmark-archives/linux-4.0.1-tar-encrypted.tar.cms -outform DER /media/bam5wi/data/workspace/zar-rs/testdata/zar-benchmark-ysqx_ie5/benchmark-cert.pem
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
409742895A7F0000:error:1C80006E:Provider routines:ossl_gcm_get_ctx_params:invalid tag:../providers/implementations/ciphers/ciphercommon_gcm.c:204:
409742895A7F0000:error:170000B9:CMS routines:ossl_cms_AuthEnvelopedData_final:cipher get tag:../crypto/cms/cms_env.c:1280:
409742895A7F0000:error:17000067:CMS routines:CMS_final:cms datafinal error:../crypto/cms/cms_smime.c:890:
</pre>
</details>
<details><summary>tar (encrypted=True, hot cache), run 2, compression: FAILED</summary>
<pre>Command failed with exit code 3: /usr/bin/openssl cms -encrypt -binary -aes-256-gcm -in /media/bam5wi/data/workspace/zar-rs/testdata/zar-benchmark-ysqx_ie5/encrypted-source.tar -out /media/bam5wi/data/workspace/zar-rs/testdata/benchmark-archives/linux-4.0.1-tar-encrypted.tar.cms -outform DER /media/bam5wi/data/workspace/zar-rs/testdata/zar-benchmark-ysqx_ie5/benchmark-cert.pem
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4027024D177F0000:error:1C80006E:Provider routines:ossl_gcm_get_ctx_params:invalid tag:../providers/implementations/ciphers/ciphercommon_gcm.c:204:
4027024D177F0000:error:170000B9:CMS routines:ossl_cms_AuthEnvelopedData_final:cipher get tag:../crypto/cms/cms_env.c:1280:
4027024D177F0000:error:17000067:CMS routines:CMS_final:cms datafinal error:../crypto/cms/cms_smime.c:890:
</pre>
</details>
<details><summary>tar (encrypted=True, cold cache), run 1, compression: FAILED</summary>
<pre>Command failed with exit code 3: /usr/bin/openssl cms -encrypt -binary -aes-256-gcm -in /media/bam5wi/data/workspace/zar-rs/testdata/zar-benchmark-ysqx_ie5/encrypted-source.tar -out /media/bam5wi/data/workspace/zar-rs/testdata/benchmark-archives/linux-4.0.1-tar-encrypted.tar.cms -outform DER /media/bam5wi/data/workspace/zar-rs/testdata/zar-benchmark-ysqx_ie5/benchmark-cert.pem
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:038C0100:memory buffer routines:BUF_MEM_grow_clean:malloc failure:../crypto/buffer/buffer.c:128:
4057DF739A7F0000:error:1C80006E:Provider routines:ossl_gcm_get_ctx_params:invalid tag:../providers/implementations/ciphers/ciphercommon_gcm.c:204:
4057DF739A7F0000:error:170000B9:CMS routines:ossl_cms_AuthEnvelopedData_final:cipher get tag:../crypto/cms/cms_env.c:1280:
4057DF739A7F0000:error:17000067:CMS routines:CMS_final:cms datafinal error:../crypto/cms/cms_smime.c:890:
</pre>
</details>

"Extract 3 files" is the time to extract only `Makefile`, `COPYING`, `kernel/sched/core.c`
from the archive (for encrypted tar/tar.gz this includes decrypting the archive). "Temp space"
is the peak disk usage of the results directory and the runner's temporary
directory during that extraction minus the size of the extracted files, sampled
every 5 ms (the maximum over all runs). Temporary files outside these
directories (e.g. in the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 4 extraction run(s), `zarlib` renamed 52 path(s) total (13 per run) due to case-folded collisions; generated suffixes are normalized only for comparison. The results
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
- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.34`
- `openssl`: `OpenSSL 3.0.2 15 Mar 2022 (Library: OpenSSL 3.0.2 15 Mar 2022)`
- `7z`: `7-Zip [64] 16.02 : Copyright (c) 1999-2016 Igor Pavlov : 2016-05-21`
- `gzip`: `gzip 1.10`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source archive and extracted source trees remain in `testdata/`.
