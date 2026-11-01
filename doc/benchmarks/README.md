# Archive benchmark overview

These reports compare `zarlib` with ZIP, 7z, tar, and tar.gz using the Linux
4.0.1 source tree (about 525 MiB). Depending on platform and available tools,
they cover archive size, compression, full extraction, extracting three
selected files, hot and cold file-cache runs, and encrypted runs. Each report
includes its own methodology, tool versions, and detailed results; its chart
visualizes the measurements available for that run.

## Reports

| Platform | System | Run date | Report | Charts |
|---|---|---|---|---|
| Linux | GitHub-hosted, `linux-arm64-gh` tag | 2026-10-30 | [Report](./benchmark-report-linux-arm64-gh.md) | [Charts](./benchmark-charts-linux-arm64-gh.svg) |
| Linux | GitHub-hosted, `linux-x86_64-gh` tag | 2026-10-30 | [Report](./benchmark-report-linux-x86_64-gh.md) | [Charts](./benchmark-charts-linux-x86_64-gh.svg) |
| Linux | Intel Core i7-10700 | 2026-11-01 | [Report](./benchmark-report-linux-x86_64-core-i7.md) | [Charts](./benchmark-charts-linux-x86_64-core-i7.svg) |
| Linux | AMD Ryzen 5 2400G | 2026-11-01 | [Report](./benchmark-report-linux-x86_64-ryzen5.md) | [Charts](./benchmark-charts-linux-x86_64-ryzen5.svg) |
| Windows | GitHub-hosted, `windows-amd64-gh` tag | 2026-10-30 | [Report](./benchmark-report-windows-amd64-gh.md) | [Charts](./benchmark-charts-windows-amd64-gh.svg) |
| Windows | Intel Core Ultra 7 265HX | 2026-11-01 | [Report](./benchmark-report-windows-x86_64-core-u7.md) | [Charts](./benchmark-charts-windows-x86_64-core-u7.svg) |

The `linux-arm64-gh` filename tag does not match the report's detected CPU
architecture (`x86_64`); check the report metadata when interpreting that run.

## How to read the results

- Compare results from the same machine and platform. CPU, storage, operating
  system, and available benchmark tools differ between runs.
- Most current hardware reports include multiple runs and show time ranges.
  Some earlier reports contain single measurements, so their results are not
  averages and should not be treated as directly comparable to repeated runs.
- Cold-cache runs require elevated permissions and may be absent when cache
  clearing is unavailable. Encrypted runs are also skipped on platforms where
  the required tool or interaction mode is unavailable.
- Extraction checks compare regular-file paths and sizes. Symlink behavior and
  other verification details are documented in each report.

The benchmark runner is [`scripts/benchmark_archivers.py`](../../scripts/benchmark_archivers.py).
The [archive benchmark workflow](../../.github/workflows/benchmark-archives.yml)
runs benchmarks and uploads reports and charts as workflow artifacts.
