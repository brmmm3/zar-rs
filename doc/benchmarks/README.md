# Archive benchmark overview

These reports compare `zarlib` with ZIP, 7z, tar, and tar.gz using the Linux
4.0.1 source tree (about 525 MiB). Depending on platform and available tools,
they cover archive size, compression, full extraction, extracting three
selected files, hot and cold file-cache runs, and encrypted runs. Each report
includes its own methodology, tool versions, and detailed results; its chart
visualizes the measurements available for that run.

## Important note to Windows benchmarks

All the Windows benchmarks are run with Windows Defender enabled. Especially when
an archive is extracted the Defender is massively slowing down the extraction
process! For comparison see Linux benchmarks.

## Running benchmarks

There are two different benchmarks: use the **zarg GUI** to try compression
settings on an existing archive, or the **comparison script** to compare
archive formats on the same Linux source tree used by the reports.

### Benchmark an archive in zarg

1. Run `cargo run --release -p zarg` from the repository root, or start an
   installed `zarg`.
2. Open a saved ZAR archive and unlock it if needed. The Benchmark action is
   unavailable for imported non-ZAR archives and while another operation runs.
3. Choose **Tools → Benchmark**. Select a block size and compression level;
   optionally enable owner/permissions and modification dates or change
   whether to include symbolic links and special files. Defaults are 8 MiB,
   level 3, metadata off, and special files on.
4. Click **Start**. The result shows regular-file count and original size,
   resulting archive size and ratio, and create/extract time and throughput.
   The ratio is archive size divided by original size; throughput uses the
   original regular-file bytes divided by elapsed time.

The GUI extracts the current archive to the system temporary directory, then
times creation and extraction of a **new, unencrypted** ZAR archive; the
initial extraction is not timed. Allow space for both extracted copies and the
new archive. Scratch files are cleaned up afterward and the original archive
is not changed. Each Start performs one measurement, not an average;
repeat it under comparable conditions. **Cancel** takes effect after the
current step finishes. This is not a comparison with other archive formats.

### Compare archive formats with the script

From the repository root, use Python 3.12 or later and a Rust toolchain.
Install the runner's Python dependencies in your chosen environment:

```sh
python -m pip install psutil scandir-rs
```

The runner needs `cargo`, `tar`, and 7-Zip (`7z`, `7zz`, or `7za`); it also
requires `zip` and `unzip` outside Windows. `openssl` enables the encrypted
tar and tar.gz cases. GNU tar's gzip support also needs `gzip`. With the
default options the script attempts to install missing external tools using a
supported system package manager, potentially requesting elevated privileges.
Install the tools yourself and pass `--no-install` to avoid that behavior.

```sh
# Hot-cache comparison; no administrator privileges or automatic installs.
python scripts/benchmark_archivers.py --mode hot --no-install

# Available cache modes and other options.
python scripts/benchmark_archivers.py --help
```

The script downloads the Linux 4.0.1 tarball if absent, validates and extracts
it under `testdata/`, and builds the release `zarc` binary before measuring.
It may refresh an existing `testdata/linux-4.0.1` tree from the tarball.
It measures ZAR, ZIP, 7z/LZMA2, tar, and tar.gz, plus 7z/Zstandard when
supported; unavailable optional cases are skipped. It runs both unencrypted
and encrypted configurations where supported. The script does not accept a
custom input dataset. Encrypted ZAR runs are skipped on Windows; 7-Zip
receives a temporary random password on its command line.

**Before running:** save anything important elsewhere. The runner empties
`testresults/` repeatedly, deletes and recreates
`testdata/benchmark-archives/`, and overwrites the report and chart for the
detected OS and architecture under `doc/benchmarks/`. The source tarball and
extracted tree remain in `testdata/`; generated archives and temporary key
material are removed at the end. Allow enough disk space for the source,
archives, extraction output, and temporary files.

`--mode hot` warms the source and archive cache before measurement. The
default `--mode both` also attempts cold-cache runs; `--mode cold` requests
only those runs. Cold cache clearing is supported on Linux with root access
to `/proc/sys/vm/drop_caches`, and on elevated Windows using Sysinternals
RAMMap. Windows may download RAMMap and requires prior acceptance of its
license or the explicit `--accept-sysinternals-eula` option. Cold runs are
skipped with a warning if privileges or support are unavailable; cold-only
mode fails if no cache mode is available. Clearing caches affects the whole
machine, not just this benchmark.

Hot configurations are measured twice and cold configurations once; the
report gives minimum and maximum times, not statistical averages. It includes
archive size, compression and full extraction time and throughput, extraction
of three selected files, temporary space, and sampled system-wide CPU/RAM (not
per-process usage). Download,
build, verification and the three-second pauses between runs are not timed.
Regular-file paths and sizes are checked after extraction, not file contents.
The report explains format-specific settings, encryption, skipped cases and
verification limitations. Compare like-for-like runs on the same hardware,
storage, cache mode and tool versions; encryption modes differ by format.

The [archive benchmark workflow](../../.github/workflows/benchmark-archives.yml)
can also be launched through GitHub Actions and uploads its reports and charts
as artifacts. Its currently enabled runners are Linux x64, Linux ARM64 and
Windows x64; it runs hot-cache comparisons.

## Reports

| Platform | System | Run date | Report | Charts |
|---|---|---|---|---|
| Linux | GitHub-hosted, ARM64 | 2026-11-08 | [Report](./benchmark-report-linux-arm64-gh.md) | [Charts](./benchmark-charts-linux-arm64-gh.svg) |
| Linux | GitHub-hosted, AMD EPYC 7763 | 2026-11-08 | [Report](./benchmark-report-linux-x86_64-gh.md) | [Charts](./benchmark-charts-linux-x86_64-gh.svg) |
| Linux | Intel Core i7-10700 | 2026-11-08 | [Report](./benchmark-report-linux-x86_64-core-i7.md) | [Charts](./benchmark-charts-linux-x86_64-core-i7.svg) |
| Linux | AMD Ryzen 5 2400G | 2026-11-08 | [Report](./benchmark-report-linux-x86_64-ryzen5.md) | [Charts](./benchmark-charts-linux-x86_64-ryzen5.svg) |
| Windows | GitHub-hosted, AMD EPYC 9V74 | 2026-11-08 | [Report](./benchmark-report-windows-x86_64-gh.md) | [Charts](./benchmark-charts-windows-x86_64-gh.svg) |
| Windows | Intel Core Ultra 7 265HX | 2026-11-09 | [Report](./benchmark-report-windows-x86_64-core-u7.md) | [Charts](./benchmark-charts-windows-x86_64-core-u7.svg) |

Run dates are shown in each report's local time zone. The Linux ARM64 report
does not identify the CPU model.

## How to read the results

- Compare results from the same machine and platform. CPU, storage, operating
  system, and available benchmark tools differ between runs.
- The reports show ranges for repeated hot-cache measurements; cold-cache
  configurations have a single measurement, not an average.
- Windows antivirus scanning may affect extraction timings; account for system
  configuration when comparing results.
- Cold-cache runs require elevated permissions and may be absent when cache
  clearing is unavailable. Encrypted runs are also skipped on platforms where
  the required tool or interaction mode is unavailable.
- Extraction checks compare regular-file paths and sizes. Symlink behavior and
  other verification details are documented in each report.
