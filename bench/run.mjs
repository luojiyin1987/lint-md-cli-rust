import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync
} from "node:fs";
import { tmpdir } from "node:os";
import os from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { performance } from "node:perf_hooks";

const benchDir = dirname(fileURLToPath(import.meta.url));
const rootDir = resolve(benchDir, "..");
const workloadsDir = join(benchDir, "workloads");

function parseArguments(args) {
  const options = {
    smoke: false,
    output: join(benchDir, "results", "benchmark.json"),
    markdown: join(benchDir, "results", "benchmark.md")
  };

  for (let index = 0; index < args.length; index += 1) {
    const value = args[index];
    if (value === "--smoke") {
      options.smoke = true;
    } else if (value === "--output") {
      options.output = resolve(args[++index] ?? "");
    } else if (value === "--markdown") {
      options.markdown = resolve(args[++index] ?? "");
    } else {
      throw new Error(`unknown argument: ${value}`);
    }
  }

  return options;
}

function positiveIntegerEnvironment(name, fallback) {
  const raw = process.env[name];
  if (raw == null || raw === "") return fallback;

  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive integer, received: ${raw}`);
  }
  return value;
}

function commandOutput(executable, args) {
  const result = spawnSync(executable, args, { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : null;
}

function formatDuration(milliseconds) {
  if (milliseconds == null) return "n/a";
  if (milliseconds < 1000) return `${milliseconds.toFixed(2)} ms`;
  return `${(milliseconds / 1000).toFixed(2)} s`;
}

function runCommand(command, repeats, timeoutMs) {
  const started = performance.now();
  for (let index = 0; index < repeats; index += 1) {
    const result = spawnSync(command.executable, command.args, {
      env: command.env,
      stdio: ["ignore", "ignore", "pipe"],
      encoding: "utf8",
      timeout: timeoutMs,
      killSignal: "SIGTERM"
    });

    if (result.error) {
      if (result.error.code === "ETIMEDOUT") {
        return {
          status: "timeout",
          elapsedMs: performance.now() - started,
          timeoutMs,
          invocation: index + 1,
          repeats
        };
      }
      throw result.error;
    }
    if (result.status !== 0) {
      throw new Error(
        `${command.name} exited with ${result.status}: ${result.stderr || "no stderr"}`
      );
    }
  }

  return { status: "completed", elapsedMs: performance.now() - started };
}

function percentile(sorted, fraction) {
  if (sorted.length === 0) return null;
  const index = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil(sorted.length * fraction) - 1)
  );
  return sorted[index];
}

function summarize(samples, repeats, bytesPerInvocation) {
  const sorted = [...samples].sort((left, right) => left - right);
  const medianMs = percentile(sorted, 0.5);
  const transferredBytes = bytesPerInvocation * repeats;
  return {
    samplesMs: samples,
    minMs: sorted[0] ?? null,
    medianMs,
    p95Ms: percentile(sorted, 0.95),
    maxMs: sorted.at(-1) ?? null,
    repeatsPerSample: repeats,
    medianPerInvocationMs: medianMs == null ? null : medianMs / repeats,
    throughputMiBPerSecond:
      medianMs != null && transferredBytes > 0
        ? transferredBytes / (1024 * 1024) / (medianMs / 1000)
        : null
  };
}

function timeoutRecord(result, phase, index = null, total = null) {
  return {
    phase,
    timeoutMs: result.timeoutMs,
    elapsedMs: result.elapsedMs,
    invocation: result.invocation,
    repeats: result.repeats,
    index,
    total
  };
}

function finishMeasurement(measurement, samples, configuration) {
  return {
    ...measurement,
    ...summarize(samples, configuration.repeats, configuration.bytes)
  };
}

function measure(command, configuration, timeoutMs) {
  const measurement = {
    status: "completed",
    firstSampleMs: null,
    timeout: null
  };
  const samples = [];

  console.log(
    `  ${command.name}: first sample ` +
      `(${configuration.repeats} invocation${configuration.repeats === 1 ? "" : "s"})`
  );
  const firstSample = runCommand(command, configuration.repeats, timeoutMs);
  if (firstSample.status === "timeout") {
    console.log(
      `    timed out after ${firstSample.timeoutMs} ms ` +
        `(invocation ${firstSample.invocation}/${firstSample.repeats})`
    );
    measurement.status = "timeout";
    measurement.timeout = timeoutRecord(firstSample, "first-sample");
    return finishMeasurement(measurement, samples, configuration);
  }
  measurement.firstSampleMs = firstSample.elapsedMs;
  console.log(`    completed in ${formatDuration(firstSample.elapsedMs)}`);

  for (let index = 0; index < configuration.warmups; index += 1) {
    console.log(`  ${command.name}: warmup ${index + 1}/${configuration.warmups}`);
    const warmup = runCommand(command, configuration.repeats, timeoutMs);
    if (warmup.status === "timeout") {
      console.log(
        `    timed out after ${warmup.timeoutMs} ms ` +
          `(invocation ${warmup.invocation}/${warmup.repeats})`
      );
      measurement.status = "timeout";
      measurement.timeout = timeoutRecord(
        warmup,
        "warmup",
        index + 1,
        configuration.warmups
      );
      return finishMeasurement(measurement, samples, configuration);
    }
    console.log(`    completed in ${formatDuration(warmup.elapsedMs)}`);
  }

  for (let index = 0; index < configuration.iterations; index += 1) {
    console.log(`  ${command.name}: sample ${index + 1}/${configuration.iterations}`);
    const sample = runCommand(command, configuration.repeats, timeoutMs);
    if (sample.status === "timeout") {
      console.log(
        `    timed out after ${sample.timeoutMs} ms ` +
          `(invocation ${sample.invocation}/${sample.repeats})`
      );
      measurement.status = "timeout";
      measurement.timeout = timeoutRecord(
        sample,
        "sample",
        index + 1,
        configuration.iterations
      );
      return finishMeasurement(measurement, samples, configuration);
    }
    samples.push(sample.elapsedMs);
    console.log(`    completed in ${formatDuration(sample.elapsedMs)}`);
  }

  return finishMeasurement(measurement, samples, configuration);
}

function peakRssKiB(command, timeoutMs) {
  if (process.platform !== "linux" || !existsSync("/usr/bin/time")) {
    return { status: "unsupported", value: null, timeout: null };
  }

  const directory = mkdtempSync(join(tmpdir(), "lint-md-bench-"));
  const output = join(directory, "time.txt");
  try {
    const result = spawnSync(
      "/usr/bin/time",
      ["-v", "-o", output, command.executable, ...command.args],
      {
        env: command.env,
        stdio: ["ignore", "ignore", "pipe"],
        encoding: "utf8",
        timeout: timeoutMs,
        killSignal: "SIGTERM"
      }
    );

    if (result.error) {
      if (result.error.code === "ETIMEDOUT") {
        return {
          status: "timeout",
          value: null,
          timeout: { phase: "peak-rss", timeoutMs }
        };
      }
      throw result.error;
    }
    if (result.status !== 0) {
      throw new Error(
        `${command.name} memory measurement exited with ${result.status}: ` +
          `${result.stderr || "no stderr"}`
      );
    }

    const report = readFileSync(output, "utf8");
    const match = /Maximum resident set size \(kbytes\):\s+(\d+)/.exec(report);
    return {
      status: match ? "completed" : "unavailable",
      value: match ? Number(match[1]) : null,
      timeout: null
    };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function attachPeakRss(measurement, command, timeoutMs, smoke) {
  if (smoke) {
    measurement.peakRssKiB = null;
    measurement.peakRssStatus = "skipped-smoke";
    return;
  }

  if (measurement.status === "timeout") {
    measurement.peakRssKiB = null;
    measurement.peakRssStatus = "skipped-timing-timeout";
    return;
  }

  console.log(`  ${command.name}: measuring peak RSS`);
  const result = peakRssKiB(command, timeoutMs);
  measurement.peakRssKiB = result.value;
  measurement.peakRssStatus = result.status;
  measurement.peakRssTimeout = result.timeout;
  if (result.status === "timeout") {
    console.log(`    peak RSS measurement timed out after ${timeoutMs} ms`);
  } else {
    console.log(`    peak RSS: ${result.value ?? "n/a"} KiB`);
  }
}

function directorySize(path) {
  if (!existsSync(path)) return null;
  let total = 0;
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const entryPath = join(path, entry.name);
    if (entry.isDirectory()) total += directorySize(entryPath) ?? 0;
    else if (entry.isFile()) total += statSync(entryPath).size;
  }
  return total;
}

function formatNumber(value, digits = 2) {
  return value == null ? "n/a" : value.toFixed(digits);
}

function formatMeasurement(measurement) {
  if (measurement.status !== "timeout") return formatNumber(measurement.medianMs);
  if (measurement.medianMs != null) {
    return `${formatNumber(measurement.medianMs)} (partial; timeout)`;
  }
  return `>${measurement.timeout.timeoutMs} (timeout)`;
}

function formatPeakRss(measurement) {
  if (measurement.peakRssStatus === "timeout") return "timeout";
  if (measurement.peakRssStatus?.startsWith("skipped")) return "skipped";
  return measurement.peakRssKiB ?? "n/a";
}

function markdownReport(report) {
  const lines = [
    "# TypeScript–Rust benchmark",
    "",
    `Status: ${report.status}`,
    "",
    `Generated: ${report.environment.generatedAt}`,
    `Updated: ${report.updatedAt}`,
    "",
    `Platform: ${report.environment.platform} ${report.environment.arch}; ${report.environment.cpuModel}; ${report.environment.cpuCount} CPUs`,
    "",
    "| Workload | Bytes | Rust result (ms) | TypeScript result (ms) | TS/Rust ratio | Rust peak RSS (KiB) | TS peak RSS (KiB) |",
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"
  ];

  for (const workload of report.workloads) {
    lines.push(
      `| ${workload.name} | ${workload.bytes} | ${formatMeasurement(workload.rust)} | ${formatMeasurement(workload.typescript)} | ${formatNumber(workload.ratio)} | ${formatPeakRss(workload.rust)} | ${formatPeakRss(workload.typescript)} |`
    );
  }

  if (report.failure) {
    lines.push("", `Failure: ${report.failure.message}`);
  }

  lines.push(
    "",
    "Timeouts are recorded as benchmark results and do not discard completed workloads.",
    "The ratio is descriptive only. Hosted-runner timing is noisy and does not gate CI.",
    "The first sample includes process startup but is not a guaranteed cold filesystem-cache measurement.",
    "Peak RSS is collected only on Linux when `/usr/bin/time` is available.",
    `Each child process is limited to ${report.environment.commandTimeoutMs} ms.`,
    ""
  );
  return lines.join("\n");
}

function persistReport(report, output, markdownPath) {
  report.updatedAt = new Date().toISOString();
  mkdirSync(dirname(output), { recursive: true });
  mkdirSync(dirname(markdownPath), { recursive: true });
  writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
  writeFileSync(markdownPath, markdownReport(report));
}

const options = parseArguments(process.argv.slice(2));
const commandTimeoutMs = positiveIntegerEnvironment(
  "LINT_MD_BENCH_COMMAND_TIMEOUT_MS",
  options.smoke ? 30_000 : 120_000
);
const rustBinary =
  process.env.LINT_MD_RS_BIN ||
  join(
    rootDir,
    "target",
    "release",
    process.platform === "win32" ? "lint-md-rs.exe" : "lint-md-rs"
  );
const typeScriptRunner = join(benchDir, "typescript-runner.mjs");
const coreReference =
  process.env.LINT_MD_CORE_REFERENCE ||
  join(rootDir, "compat", "core-reference", "lib", "index.js");

for (const required of [rustBinary, typeScriptRunner, coreReference]) {
  if (!existsSync(required)) {
    throw new Error(`required benchmark input is missing: ${required}`);
  }
}

const definitions = options.smoke
  ? [
      { name: "startup-tiny", file: "tiny.md", iterations: 3, warmups: 1, repeats: 1 },
      { name: "medium", file: "medium.md", iterations: 1, warmups: 0, repeats: 1 },
      { name: "small-files-batch", file: "tiny.md", iterations: 1, warmups: 0, repeats: 5 }
    ]
  : [
      { name: "startup-tiny", file: "tiny.md", iterations: 12, warmups: 2, repeats: 1 },
      { name: "medium", file: "medium.md", iterations: 3, warmups: 1, repeats: 1 },
      { name: "large", file: "large.md", iterations: 1, warmups: 0, repeats: 1 },
      { name: "small-files-batch", file: "tiny.md", iterations: 3, warmups: 1, repeats: 20 }
    ];

const report = {
  schemaVersion: 2,
  status: "running",
  updatedAt: new Date().toISOString(),
  mode: options.smoke ? "smoke" : "full",
  environment: {
    generatedAt: new Date().toISOString(),
    platform: process.platform,
    release: os.release(),
    arch: process.arch,
    cpuModel: os.cpus()[0]?.model ?? "unknown",
    cpuCount: os.cpus().length,
    totalMemoryBytes: os.totalmem(),
    nodeVersion: process.version,
    rustcVersion: commandOutput("rustc", ["--version"]),
    repositoryCommit: commandOutput("git", ["rev-parse", "HEAD"]),
    coreReference,
    commandTimeoutMs
  },
  distribution: null,
  workloads: [],
  failure: null
};

persistReport(report, options.output, options.markdown);
console.log(
  `Running ${options.smoke ? "smoke" : "full"} benchmark with ` +
    `${definitions.length} workloads and a ${commandTimeoutMs} ms child-process timeout.`
);

try {
  for (
    let definitionIndex = 0;
    definitionIndex < definitions.length;
    definitionIndex += 1
  ) {
    const definition = definitions[definitionIndex];
    const file = join(workloadsDir, definition.file);
    if (!existsSync(file)) {
      throw new Error(
        `workload missing: ${file}; run node bench/generate-workloads.mjs`
      );
    }

    const bytes = statSync(file).size;
    const configuration = { ...definition, bytes };
    const sharedEnvironment = {
      ...process.env,
      LINT_MD_CORE_REFERENCE: coreReference
    };
    const rustCommand = {
      name: `Rust ${definition.name}`,
      executable: rustBinary,
      args: ["--format", "json", file],
      env: sharedEnvironment
    };
    const typeScriptCommand = {
      name: `TypeScript ${definition.name}`,
      executable: process.execPath,
      args: [typeScriptRunner, file],
      env: sharedEnvironment
    };

    console.log(
      `\n[${definitionIndex + 1}/${definitions.length}] ${definition.name} ` +
        `(${bytes} bytes; ${definition.iterations} samples; ` +
        `${definition.repeats} invocation${definition.repeats === 1 ? "" : "s"} per sample)`
    );

    const rust = measure(rustCommand, configuration, commandTimeoutMs);
    const typescript = measure(typeScriptCommand, configuration, commandTimeoutMs);
    attachPeakRss(rust, rustCommand, commandTimeoutMs, options.smoke);
    attachPeakRss(
      typescript,
      typeScriptCommand,
      commandTimeoutMs,
      options.smoke
    );

    const ratio =
      rust.status === "completed" &&
      typescript.status === "completed" &&
      rust.medianMs != null &&
      typescript.medianMs != null
        ? typescript.medianMs / rust.medianMs
        : null;

    report.workloads.push({
      name: definition.name,
      file: definition.file,
      bytes,
      iterations: definition.iterations,
      warmups: definition.warmups,
      repeatsPerSample: definition.repeats,
      status:
        rust.status === "timeout" || typescript.status === "timeout"
          ? "completed-with-timeout"
          : "completed",
      rust,
      typescript,
      ratio
    });
    persistReport(report, options.output, options.markdown);

    console.log(
      `  completed ${definition.name}: Rust ${formatMeasurement(rust)}, ` +
        `TypeScript ${formatMeasurement(typescript)}`
    );
  }

  if (!options.smoke) {
    console.log("\nCollecting distribution sizes.");
    report.distribution = {
      rustBinaryBytes: statSync(rustBinary).size,
      typeScriptBuildBytes: directorySize(dirname(coreReference)),
      typeScriptNodeModulesBytes: directorySize(
        join(rootDir, "compat", "core-reference", "node_modules")
      )
    };
  }

  report.status = report.workloads.some(
    (workload) => workload.status === "completed-with-timeout"
  )
    ? "completed-with-timeouts"
    : "completed";
  persistReport(report, options.output, options.markdown);
} catch (error) {
  report.status = "failed";
  report.failure = {
    message: error instanceof Error ? error.message : String(error),
    stack: error instanceof Error ? error.stack : null
  };
  persistReport(report, options.output, options.markdown);
  throw error;
}

const markdown = markdownReport(report);
console.log(`\n${markdown}`);
console.log(`JSON report: ${options.output}`);
console.log(`Markdown report: ${options.markdown}`);
