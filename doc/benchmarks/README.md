# Archive benchmark overview

These reports compare `zarlib` with ZIP, 7z, tar, and tar.gz across five
datasets: a small Linux source tree, icons, duplicate icons, the Linux 4.0.1
source tree (about 525 MiB), and a precompiled Linux 6.1.1 kernel tree.
Depending on platform and available tools, they cover archive size,
compression, full extraction, extracting three selected files, hot and cold
file-cache runs, and encrypted runs. Each report includes its own methodology,
tool versions, and detailed results; its chart visualizes the measurements
available for that run. Older reports may use different datasets and methods.

## Important note to Windows benchmarks

All the Windows benchmarks are run with Windows Defender enabled. Especially
when an archive is extracted the Defender is massively slowing down the
extraction process! For comparison see Linux benchmarks.

## Running benchmarks

There are two different benchmarks: use the **zarg GUI** to try compression
settings on an existing archive, or the **comparison script** to compare
archive formats on the supplied datasets.

### Benchmark an archive in zarg

1. Run `cargo run --release -p zarlib-gui` from the repository root, or start an
   installed `zarg`.
2. Open a saved ZAR archive and unlock it if needed. The Benchmark action is
   unavailable for imported non-ZAR archives and while another operation runs.
3. Choose **Tools → Benchmark**. Select a block size and compression level;
   optionally enable owner/permissions and modification times or change
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

# Run just one packaged dataset.
python scripts/benchmark_archivers.py --dataset icons --mode hot --no-install

# Available cache modes and other options.
python scripts/benchmark_archivers.py --help
```

By default, the script builds the release `zarc` binary, then validates and
extracts all five packaged ZAR datasets under `testresults/datasets/`. Use
`--dataset` to select one dataset: `linux-4.0.1`, `linux-6.1.1`, `duplicates`,
`icons`, or `small_archive`. Only the selected dataset is prepared and
benchmarked. Missing archives or Git LFS pointer files are downloaded into
`testdata/`; existing extracted trees are reused when their source archive
hash matches.
It measures ZAR, ZIP, 7z/LZMA2, tar, and tar.gz, plus 7z/Zstandard when
supported; unavailable optional cases are skipped. It runs both unencrypted
and encrypted configurations where supported. The script does not accept a
custom input path. The precompiled Linux 6.1.1 dataset includes build outputs
and does not require a local kernel build. The runner produces a separate
report and chart for each selected dataset. 7-Zip receives a temporary random
password on its command line.

Console progress includes the current benchmark run and total, compression,
full and selected-file extraction, verification, and the completed percentage.
Long-running steps print elapsed-time updates every five seconds, including
source preparation and the release build. Progress setup and teardown are outside
the timed operations; benchmark result timings remain separate from step elapsed time.
If an archiver command fails, the runner records the error and continues with
the other runs. A configuration with any failed repetition is marked `FAILED`
in the report and chart, with error details in the report. Failure to create the
temporary certificate marks encrypted tar cases as failed; other setup, cache,
and verification errors still stop the runner. After completing the selected
datasets and writing their reports, the script exits nonzero if any
configuration failed.

Each verified or failed benchmark run is saved atomically in the repository's
`.benchmark_cache/` directory, including individual hot-cache repetitions and
separate entries for each dataset.
Filenames start with the overall progress number, for example
`001-zarlib-plain-hot-run-1.json`. Restarting the script reuses completed runs,
including resource samples, and only measures missing runs. Cached runs are found
by test name even if their progress number changes. Source preparation and the
release build still run on restart.

The script hashes source file contents and records paths, metadata, and symlink
targets before each dataset. It rejects malformed cache files or results with a
mismatched platform, source fingerprint, or tool versions; remove those files to
measure the affected tests again. Existing cache entries from older versions of
the runner must also be removed. Delete all cached files to start an entirely
fresh benchmark.
Cache writes are outside timed operations. If execution stops before all reports
are generated, completed results remain available. Once all reports and charts
for the invocation are written, its cache entries are cleared, including when a
report records failed cases. A normal run clears the whole cache; a `--dataset`
run clears only that dataset's entries and preserves cached results for the
other datasets.

**Before running:** save anything important elsewhere. The runner empties
the extraction output in `testresults/` repeatedly, deletes and recreates
`testdata/benchmark-archives/`, and overwrites the report and chart for the
detected OS, architecture, CPU, and dataset under `doc/benchmarks/`. The source
archives remain in `testdata/` and extracted datasets in `testresults/datasets/`;
generated archives and temporary key material are removed at the end. Allow
enough disk space for both source trees, kernel build outputs, archives,
extraction output, and temporary files.

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
of three selected files, temporary space, sampled system-wide CPU utilisation,
and the summed resident memory (RSS) of the archiver and its child processes.
Shared memory can be counted more than once, and operations shorter than the
sampling interval may have no resource samples. Older reports used system-wide
RAM rather than process RSS and should not be compared directly. Download,
build, verification and the three-second pauses between runs are not timed.
Regular-file paths and sizes are checked after extraction, not file contents.
On Windows, one missing regular file is tolerated; unexpected files, multiple
missing files, and size mismatches still fail verification.
The report explains format-specific settings, encryption, skipped cases and
verification limitations. Compare like-for-like runs on the same hardware,
storage, cache mode and tool versions; encryption modes differ by format.

The [archive benchmark workflow](../../.github/workflows/benchmark-archives.yml)
can also be launched through GitHub Actions. Its matrix runs each of the five
datasets separately on every enabled runner (Linux x64, Linux ARM64, and
Windows x64), with platform/dataset jobs running in parallel. Each job runs
hot-cache comparisons and uploads a uniquely named report-and-chart artifact.
Use the **Dataset** and **Operating system** workflow inputs to limit the run;
both default to **all** when left unselected. Choosing Linux selects both Linux
runners.

## Reports

Report and chart filenames include the operating system, architecture, and
dataset, followed by a recognized CPU suffix (`-ryzen5`, `-core-i7`, or
`-core-u7` for Core U7 and Core Ultra 7 CPUs). GitHub Actions runs add `-gh`
last. For example, Linux 4.0.1 results on an Intel Core i7 are named
`benchmark-report-linux-x86_64-linux-4.0.1-core-i7.md` and
`benchmark-charts-linux-x86_64-linux-4.0.1-core-i7.svg`. Historical reports
may retain earlier naming conventions.

| Dataset | Platform / system (run date) | Report | Chart |
| --- | --- | --- | --- |
| Linux 4.0.1 | Linux x86_64, Core i7-10700 (2026-11-20) | [Report](./benchmark-report-linux-x86_64-core-i7.md) | [Chart](./benchmark-charts-linux-x86_64-core-i7.svg) |
| Linux 4.0.1 | Linux x86_64, Ryzen 5 2400G (2026-11-18) | [Report](./benchmark-report-linux-x86_64-ryzen5.md) | [Chart](./benchmark-charts-linux-x86_64-ryzen5.svg) |
| Linux 4.0.1 | Linux x86_64, GitHub-hosted, AMD EPYC 7763 (2026-11-08) | [Report](./benchmark-report-linux-x86_64-gh.md) | [Chart](./benchmark-charts-linux-x86_64-gh.svg) |
| Linux 4.0.1 | Linux ARM64, GitHub-hosted, CPU not reported (2026-11-08) | [Report](./benchmark-report-linux-arm64-gh.md) | [Chart](./benchmark-charts-linux-arm64-gh.svg) |
| Linux 4.0.1 | Windows x86_64, Core Ultra 7 265HX (2026-11-20) | [Report](./benchmark-report-windows-x86_64-core-u7.md) | [Chart](./benchmark-charts-windows-x86_64-core-u7.svg) |
| Linux 4.0.1 | Windows x86_64, GitHub-hosted, AMD EPYC 9V74 (2026-11-08) | [Report](./benchmark-report-windows-x86_64-gh.md) | [Chart](./benchmark-charts-windows-x86_64-gh.svg) |
| Linux 4.0.1 | Windows x86_64, Core Ultra 7 265HX (2026-11-19) | [Report](./benchmark-report-windows-x86_64.md) | [Chart](./benchmark-charts-windows-x86_64.svg) |
| Linux 4.0.1 | Windows AMD64, GitHub-hosted, CPU model not reported (2026-10-30) | [Report](./benchmark-report-windows-amd64-gh.md) | [Chart](./benchmark-charts-windows-amd64-gh.svg) |
| Linux 6.1.1 precompiled | Linux x86_64, Ryzen 5 2400G (2026-11-20) | [Report](./benchmark-report-linux-x86_64-linux-6.1.1-ryzen5.md) | [Chart](./benchmark-charts-linux-x86_64-linux-6.1.1-ryzen5.svg) |
| Linux 6.1.1 precompiled | Linux x86_64, Core i7-10700 (2026-11-20) | [Report](./benchmark-report-linux-x86_64-linux-6.1.1-core-i7.md) | [Chart](./benchmark-charts-linux-x86_64-linux-6.1.1-core-i7.svg) |
| Linux 6.1.1 precompiled | Windows x86_64, Core Ultra 7 265HX (2026-11-20) | [Report](./benchmark-report-windows-x86_64-linux-6.1.1-core-u7.md) | [Chart](./benchmark-charts-windows-x86_64-linux-6.1.1-core-u7.svg) |
| Duplicate icons | Linux x86_64, Core i7-10700 (2026-11-20) | [Report](./benchmark-report-linux-x86_64-duplicates-core-i7.md) | [Chart](./benchmark-charts-linux-x86_64-duplicates-core-i7.svg) |
| Duplicate icons | Linux x86_64, Ryzen 5 2400G (2026-11-20) | [Report](./benchmark-report-linux-x86_64-duplicates-ryzen5.md) | [Chart](./benchmark-charts-linux-x86_64-duplicates-ryzen5.svg) |
| Duplicate icons | Windows x86_64, Core Ultra 7 265HX (2026-11-20) | [Report](./benchmark-report-windows-x86_64-duplicates-core-u7.md) | [Chart](./benchmark-charts-windows-x86_64-duplicates-core-u7.svg) |
| Icons | Linux x86_64, Core i7-10700 (2026-11-20) | [Report](./benchmark-report-linux-x86_64-icons-core-i7.md) | [Chart](./benchmark-charts-linux-x86_64-icons-core-i7.svg) |
| Icons | Linux x86_64, Ryzen 5 2400G (2026-11-20) | [Report](./benchmark-report-linux-x86_64-icons-ryzen5.md) | [Chart](./benchmark-charts-linux-x86_64-icons-ryzen5.svg) |
| Icons | Windows x86_64, Core Ultra 7 265HX (2026-11-20) | [Report](./benchmark-report-windows-x86_64-icons-core-u7.md) | [Chart](./benchmark-charts-windows-x86_64-icons-core-u7.svg) |
| Small archive | Linux x86_64, Core i7-10700 (2026-11-20) | [Report](./benchmark-report-linux-x86_64-small_archive-core-i7.md) | [Chart](./benchmark-charts-linux-x86_64-small_archive-core-i7.svg) |
| Small archive | Linux x86_64, Ryzen 5 2400G (2026-11-20) | [Report](./benchmark-report-linux-x86_64-small_archive-ryzen5.md) | [Chart](./benchmark-charts-linux-x86_64-small_archive-ryzen5.svg) |
| Small archive | Windows x86_64, Core Ultra 7 265HX (2026-11-20) | [Report](./benchmark-report-windows-x86_64-small_archive-core-u7.md) | [Chart](./benchmark-charts-windows-x86_64-small_archive-core-u7.svg) |

Run dates are shown in each report's local time zone. The Linux ARM64 report
does not identify the CPU model. For the newer Linux and Windows reports on all
five datasets, see the [dataset report index](../../README.md#benchmarks).

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
- 7-Zip may reject source symlinks whose relative targets contain parent
  components. The runner accepts only these link errors, up to the number of
  source symlinks; all regular files are still checked after extraction.
