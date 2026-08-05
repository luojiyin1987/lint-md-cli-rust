import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { runReference } from "./reference.mjs";

const compatDir = dirname(fileURLToPath(import.meta.url));
const rootDir = resolve(compatDir, "..");
const manifest = JSON.parse(readFileSync(join(compatDir, "fixtures.json"), "utf8"));
const expected = JSON.parse(
  readFileSync(join(compatDir, "expected-mismatches.json"), "utf8")
);

const defaultBinary = join(
  rootDir,
  "target",
  "debug",
  process.platform === "win32" ? "lint-md-rs.exe" : "lint-md-rs"
);
const binary = process.env.LINT_MD_RS_BIN || defaultBinary;

function normalizeDiagnostics(diagnostics) {
  return [...diagnostics]
    .map((item) => ({
      ruleId: item.ruleId,
      line: item.line,
      column: item.column,
      severity: item.severity,
      fixable: Boolean(item.fixable)
    }))
    .sort((left, right) =>
      left.line - right.line ||
      left.column - right.column ||
      left.ruleId.localeCompare(right.ruleId)
    );
}

function runRust(content) {
  const execution = spawnSync(
    binary,
    ["--stdin", "--fix", "--format", "json"],
    { input: content, encoding: "utf8" }
  );

  if (execution.error) {
    throw execution.error;
  }
  if (![0, 1].includes(execution.status)) {
    throw new Error(
      `Rust runner failed with ${execution.status}: ${execution.stderr}`
    );
  }

  let payload;
  try {
    payload = JSON.parse(execution.stdout.trim());
  } catch (error) {
    throw new Error(`Invalid Rust JSON: ${execution.stdout}\n${error}`);
  }

  return {
    diagnostics: normalizeDiagnostics(payload.diagnostics),
    fixedContent: payload.fixedContent,
    changed: Boolean(payload.changed),
    exitCode: execution.status
  };
}

function diagnosticProjection(result, fields) {
  return result.diagnostics.map((item) =>
    Object.fromEntries(fields.map((field) => [field, item[field]]))
  );
}

function same(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function mismatchCategories(reference, rust) {
  const categories = [];

  if (reference.diagnostics.length !== rust.diagnostics.length) {
    categories.push("diagnostic-count");
  }
  if (
    !same(
      diagnosticProjection(reference, ["ruleId"]),
      diagnosticProjection(rust, ["ruleId"])
    )
  ) {
    categories.push("rule-ids");
  }
  if (
    !same(
      diagnosticProjection(reference, ["ruleId", "line", "column"]),
      diagnosticProjection(rust, ["ruleId", "line", "column"])
    )
  ) {
    categories.push("locations");
  }
  if (
    !same(
      diagnosticProjection(reference, ["ruleId", "fixable"]),
      diagnosticProjection(rust, ["ruleId", "fixable"])
    )
  ) {
    categories.push("fixability");
  }
  if (reference.fixedContent !== rust.fixedContent) {
    categories.push("fixed-output");
  }
  if (reference.exitCode !== rust.exitCode) {
    categories.push("exit-code");
  }

  return categories.sort();
}

let failed = false;
let exactMatches = 0;
let acceptedMismatches = 0;
const fixtureIds = new Set(manifest.map((fixture) => fixture.id));

for (const baselineId of Object.keys(expected)) {
  if (!fixtureIds.has(baselineId)) {
    console.error(`STALE BASELINE: ${baselineId} has no fixture`);
    failed = true;
  }
}

for (const fixture of manifest) {
  const content = readFileSync(join(compatDir, fixture.path), "utf8");
  const reference = runReference(content);
  const rust = runRust(content);
  const actual = mismatchCategories(reference, rust);
  const baseline = [...(expected[fixture.id] ?? [])].sort();

  if (same(actual, baseline)) {
    if (actual.length === 0) {
      exactMatches += 1;
      console.log(`MATCH ${fixture.id}`);
    } else {
      acceptedMismatches += 1;
      console.log(`KNOWN ${fixture.id}: ${actual.join(", ")}`);
    }
    continue;
  }

  failed = true;
  console.error(`MISMATCH ${fixture.id}`);
  console.error(`  expected: ${baseline.join(", ") || "exact match"}`);
  console.error(`  actual:   ${actual.join(", ") || "exact match"}`);
  console.error(`  TypeScript: ${JSON.stringify(reference, null, 2)}`);
  console.error(`  Rust:       ${JSON.stringify(rust, null, 2)}`);
}

console.log(
  `\n${manifest.length} fixtures: ${exactMatches} exact, ${acceptedMismatches} known mismatches`
);

if (failed) process.exit(1);
