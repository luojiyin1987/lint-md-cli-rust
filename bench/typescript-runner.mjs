import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const benchDir = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const corePath =
  process.env.LINT_MD_CORE_REFERENCE ||
  join(benchDir, "..", "compat", "core-reference", "lib", "index.js");
const core = require(corePath);
const { lintMarkdown } = core;

if (typeof lintMarkdown !== "function") {
  throw new Error(`TypeScript reference at ${corePath} does not expose lintMarkdown`);
}

const ALL_RULES = [
  "space-around-alphabet",
  "no-empty-list",
  "no-empty-code",
  "no-empty-code-lang",
  "no-empty-inline-code",
  "no-empty-url",
  "no-full-width-number",
  "no-long-code",
  "no-multiple-space-blockquote",
  "no-space-in-inline-code",
  "no-space-in-link",
  "no-special-characters",
  "space-around-number",
  "use-standard-ellipsis",
  "correct-title-trailing-punctuation",
  "no-empty-blockquote",
  "no-half-width-punctuation",
  "require-trailing-spaces",
  "space-around-link",
  "no-multiple-blank-lines"
];

const PROTOTYPE_RULES = new Set([
  "no-full-width-number",
  "no-empty-inline-code",
  "no-empty-blockquote",
  "no-multiple-space-blockquote",
  "no-multiple-blank-lines"
]);

const rules = Object.fromEntries(
  ALL_RULES.map((ruleId) => [ruleId, PROTOTYPE_RULES.has(ruleId) ? 2 : 0])
);

const file = process.argv[2];
if (!file) {
  console.error("usage: node bench/typescript-runner.mjs <markdown-file>");
  process.exit(2);
}

const content = readFileSync(file, "utf8");
const result = lintMarkdown(content, rules, false);
const diagnostics = Array.isArray(result?.diagnostics)
  ? result.diagnostics
  : Array.isArray(result?.lintResult)
    ? result.lintResult
    : null;

if (!diagnostics) {
  throw new Error("Unsupported @lint-md/core result shape");
}

process.stdout.write(`${JSON.stringify({ diagnostics: diagnostics.length })}\n`);
process.exitCode = diagnostics.length > 0 ? 1 : 0;
