# Reproducible TypeScript–Rust benchmark

This harness measures the Rust prototype and the pinned TypeScript `@lint-md/core` reference with deterministic Markdown workloads. It is intended to answer whether native startup, throughput, peak memory, and distribution size justify expanding the rewrite. It does not enforce performance thresholds in CI.

## Fairness boundaries

- Both implementations enable only the five prototype rules.
- Generated workloads are clean under both implementations, so output volume and known compatibility differences do not dominate timing.
- Rust is built in release mode for full measurements.
- The TypeScript reference is pinned to commit `9c88a15d43a9dd8637c57157774214a379807d26` (`2.3.0`).
- Reports include environment metadata and raw samples.
- Hosted-runner results are descriptive and do not gate pull requests.

## Prepare the TypeScript reference

From the repository root:

```bash
gh repo clone luojiyin1987/lint-md compat/core-reference
git -C compat/core-reference checkout 9c88a15d43a9dd8637c57157774214a379807d26
npm install --prefix compat/core-reference --ignore-scripts --no-audit --no-fund
npm run build --prefix compat/core-reference
```

## Run a full benchmark

```bash
cargo build --release
node bench/generate-workloads.mjs
node bench/run.mjs
```

The harness writes `bench/results/benchmark.json` with environment data and raw samples, plus `bench/results/benchmark.md` with a compact comparison table. Generated workloads and reports are ignored by Git.

## Smoke mode

```bash
cargo build
node bench/generate-workloads.mjs
LINT_MD_RS_BIN=target/debug/lint-md-rs node bench/run.mjs --smoke
```

On Windows, set `LINT_MD_RS_BIN` to `target\\debug\\lint-md-rs.exe`.

## Measurements

Full mode includes a 4 KiB startup workload, 1 MiB and 5 MiB throughput workloads, and a process-per-file batch of 50 small files. On Linux it also records peak RSS through `/usr/bin/time -v`. Distribution data includes Rust binary size, TypeScript build size, and TypeScript dependency size.

`firstSampleMs` includes process startup but is not a guaranteed cold filesystem-cache measurement. Compare reports only when their recorded environments are sufficiently similar.

## GitHub Actions

Normal CI runs smoke mode to detect broken scripts. The separate **Benchmark** workflow is manually triggered and uploads JSON and Markdown reports as artifacts. It intentionally does not fail because one implementation becomes slower by a noisy percentage.
