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
        throw new Error(
          `${command.name} timed out after ${timeoutMs} ms ` +
            `(invocation ${index + 1}/${repeats})`
        );
      }
      throw result.error;
    }
    if (result.status !== 0) {
      throw new Error(
        `${command.name} exited with ${result.status}: ${result.stderr || "no stderr"}`
      );
    }
  }
  return performance.now() - started;
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
    minMs: sorted[0],
    medianMs,
    p95Ms: percentile(sorted, 0.95),
    maxMs: sorted.at(-1),
    repeatsPerSample: repeats,
    medianPerInvocationMs: medianMs / repeats,
    throughputMiBPerSecond:
      transferredBytes > 0
        ? transferredBytes / (1024 * 1024) / (medianMs / 1000)
        : null
  };
}

function measure(command, configuration, timeoutMs) {
  console.log(
    `  ${command.name}: first sample ` +
      `(${configuration.repeats} invocation${configuration.repeats === 1 ? "" : "s"})`
  );
  const firstSampleMs = runCommand(command, configuration.repeats, timeoutMs);
  console.log(`    completed in ${formatDuration(firstSampleMs)}`);

  for (let index = 0; index < configuration.warmups; index += 1) {
    console.log(`  ${command.name}: warmup ${index + 1}/${configuration.warmups}`);
    const elapsed = runCommand(command, configuration.repeats, timeoutMs);
    console.log(`    completed in ${formatDuration(elapsed)}`);
  }

  const samples = [];
  for (let index = 0; index < configuration.iterations; index += 1) {
    console.log(
      `  ${command.name}: sample ${index + 1}/${configuration.iterations}`
    );
    const elapsed = runCommand(command, configuration.repeats, timeoutMs);
    samples.push(elapsed);
    console.log(`    completed in ${formatDuration(elapsed)}`);
  }

  return {
    firstSampleMs,
    ...summarize(samples, configuration.repeats, configuration.bytes)
  };
}

function peakRssKiB(command, timeoutMs) {
  if (process.platform !== "linux" || !existsSync("/usr/bin/time")) return null;

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
        throw new Error(
          `${command.name} memory measurement timed out after ${timeoutMs} ms`
        );
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
    return match ? Number(match[1]) : null;
  } finally {
    rmSync(directory, { recursive: true, force: true });
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

function markdownReport(report) {
  const lines = [
    "# TypeScript–Rust benchmark",
    "",
    `Generated: ${report.environment.generatedAt}`,
    "",
    `Platform: ${report.environment.platform} ${report.environment.arch}; ${report.environment.cpuModel}; ${report.environment.cpuCount} CPUs`,
    "",
    "| Workload | Bytes | Rust median (ms) | TypeScript median (ms) | TS/Rust ratio | Rust peak RSS (KiB) | TS peak RSS (KiB) |",
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"
  ];

  for (const workload of report.workloads) {
    lines.push(
      `| ${workload.name} | ${workload.bytes} | ${formatNumber(workload.rust.medianMs)} | ${formatNumber(workload.typescript.medianMs)} | ${formatNumber(workload.ratio)} | ${workload.rust.peakRssKiB ?? "n/a"} | ${workload.typescript.peakRssKiB ?? "n/a"} |`
    );
  }

  lines.push(
    "",
    "The ratio is descriptive only. Hosted-runner timing is noisy and does not gate CI.",
    "The first sample includes process startup but is not a guaranteed cold filesystem-cache measurement.",
    "Peak RSS is collected only on Linux when `/usr/bin/time` is available.",
    `Each child process is limited to ${report.environment.commandTimeoutMs} ms.`,
    ""
  );
  return lines.join("\n");
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
      {
        name: "startup-tiny",
        file: "tiny.md",
        iterations: 3,
        warmups: 1,
        repeats: 1
      },
      {
        name: "medium",
        file: "medium.md",
        iterations: 1,
        warmups: 0,
        repeats: 1
      },
      {
        name: "small-files-batch",
        file: "tiny.md",
        iterations: 1,
        warmups: 0,
        repeats: 5
      }
    ]
  : [
      {
        name: "startup-tiny",
        file: "tiny.md",
        iterations: 12,
        warmups: 2,
        repeats: 1
      },
      {
        name: "medium",
        file: "medium.md",
        iterations: 3,
        warmups: 1,
        repeats: 1
      },
      {
        name: "large",
        file: "large.md",
        iterations: 1,
        warmups: 0,
        repeats: 1
      },
      {
        name: "small-files-batch",
        file: "tiny.md",
        iterations: 3,
        warmups: 1,
        repeats: 20
      }
    ];

console.log(
  `Running ${options.smoke ? "smoke" : "full"} benchmark with ` +
    `${definitions.length} workloads and a ${commandTimeoutMs} ms child-process timeout.`
);

const workloads = [];
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
  const typescript = measure(
    typeScriptCommand,
    configuration,
    commandTimeoutMs
  );

  if (!options.smoke) {
    console.log(`  ${rustCommand.name}: measuring peak RSS`);
    rust.peakRssKiB = peakRssKiB(rustCommand, commandTimeoutMs);
    console.log(`    peak RSS: ${rust.peakRssKiB ?? "n/a"} KiB`);

    console.log(`  ${typeScriptCommand.name}: measuring peak RSS`);
    typescript.peakRssKiB = peakRssKiB(
      typeScriptCommand,
      commandTimeoutMs
    );
    console.log(`    peak RSS: ${typescript.peakRssKiB ?? "n/a"} KiB`);
  } else {
    rust.peakRssKiB = null;
    typescript.peakRssKiB = null;
  }

  workloads.push({
    name: definition.name,
    file: definition.file,
    bytes,
    iterations: definition.iterations,
    warmups: definition.warmups,
    repeatsPerSample: definition.repeats,
    rust,
    typescript,
    ratio: typescript.medianMs / rust.medianMs
  });

  console.log(
    `  completed ${definition.name}: Rust ${formatDuration(rust.medianMs)}, ` +
      `TypeScript ${formatDuration(typescript.medianMs)}`
  );
}

if (!options.smoke) {
  console.log("\nCollecting distribution sizes.");
}

const report = {
  schemaVersion: 1,
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
  distribution: options.smoke
    ? null
    : {
        rustBinaryBytes: statSync(rustBinary).size,
        typeScriptBuildBytes: directorySize(dirname(coreReference)),
        typeScriptNodeModulesBytes: directorySize(
          join(rootDir, "compat", "core-reference", "node_modules")
        )
      },
  workloads
};

mkdirSync(dirname(options.output), { recursive: true });
mkdirSync(dirname(options.markdown), { recursive: true });
writeFileSync(options.output, `${JSON.stringify(report, null, 2)}\n`);
const markdown = markdownReport(report);
writeFileSync(options.markdown, markdown);
console.log(`\n${markdown}`);
console.log(`JSON report: ${options.output}`);
console.log(`Markdown report: ${options.markdown}`);
