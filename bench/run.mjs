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

function commandOutput(executable, args) {
  const result = spawnSync(executable, args, { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : null;
}

function runCommand(command, repeats = 1) {
  const started = performance.now();
  for (let index = 0; index < repeats; index += 1) {
    const result = spawnSync(command.executable, command.args, {
      env: command.env,
      stdio: ["ignore", "ignore", "pipe"],
      encoding: "utf8"
    });
    if (result.error) throw result.error;
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

function measure(command, configuration) {
  const firstSampleMs = runCommand(command, configuration.repeats);
  for (let index = 0; index < configuration.warmups; index += 1) {
    runCommand(command, configuration.repeats);
  }

  const samples = [];
  for (let index = 0; index < configuration.iterations; index += 1) {
    samples.push(runCommand(command, configuration.repeats));
  }

  return {
    firstSampleMs,
    ...summarize(samples, configuration.repeats, configuration.bytes)
  };
}

function peakRssKiB(command) {
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
        encoding: "utf8"
      }
    );
    if (result.error) throw result.error;
    if (result.status !== 0) {
      throw new Error(
        `${command.name} memory measurement exited with ${result.status}: ${result.stderr || "no stderr"}`
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
    ""
  );
  return lines.join("\n");
}

const options = parseArguments(process.argv.slice(2));
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
      { name: "startup-tiny", file: "tiny.md", iterations: 30, warmups: 5, repeats: 1 },
      { name: "medium", file: "medium.md", iterations: 8, warmups: 2, repeats: 1 },
      { name: "large", file: "large.md", iterations: 3, warmups: 1, repeats: 1 },
      { name: "small-files-batch", file: "tiny.md", iterations: 3, warmups: 1, repeats: 50 }
    ];

const workloads = [];
for (const definition of definitions) {
  const file = join(workloadsDir, definition.file);
  if (!existsSync(file)) {
    throw new Error(`workload missing: ${file}; run node bench/generate-workloads.mjs`);
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

  const rust = measure(rustCommand, configuration);
  const typescript = measure(typeScriptCommand, configuration);
  if (!options.smoke) {
    rust.peakRssKiB = peakRssKiB(rustCommand);
    typescript.peakRssKiB = peakRssKiB(typeScriptCommand);
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
    coreReference
  },
  distribution: options.smoke
    ? null
    : {
        rustBinaryBytes: statSync(rustBinary).size,
        typeScriptBuildBytes: directorySize(join(dirname(coreReference))),
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
console.log(markdown);
console.log(`JSON report: ${options.output}`);
console.log(`Markdown report: ${options.markdown}`);
