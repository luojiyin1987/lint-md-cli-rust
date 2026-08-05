import * as coreModule from "@lint-md/core";

const core = coreModule.default ?? coreModule;
const lintMarkdown = coreModule.lintMarkdown ?? core.lintMarkdown;
const fixMarkdown = coreModule.fixMarkdown ?? core.fixMarkdown;

if (typeof lintMarkdown !== "function" || typeof fixMarkdown !== "function") {
  throw new Error("@lint-md/core does not expose lintMarkdown and fixMarkdown");
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

function severityName(value) {
  if (value === 2 || value === "error") return "error";
  if (value === 1 || value === "warning" || value === "warn") return "warning";
  return String(value ?? "error");
}

function normalizeDiagnostics(result) {
  if (Array.isArray(result?.diagnostics)) {
    return result.diagnostics.map((item) => ({
      ruleId: item.ruleId,
      line: item.line,
      column: item.column,
      severity: severityName(item.severity),
      fixable: PROTOTYPE_RULES.has(item.ruleId)
    }));
  }

  if (Array.isArray(result?.lintResult)) {
    return result.lintResult.map((item) => ({
      ruleId: item.name,
      line: item.loc.start.line,
      column: item.loc.start.column,
      severity: severityName(item.severity),
      fixable: PROTOTYPE_RULES.has(item.name)
    }));
  }

  throw new Error("Unsupported @lint-md/core result shape");
}

function sortDiagnostics(diagnostics) {
  return diagnostics.sort((left, right) =>
    left.line - right.line ||
    left.column - right.column ||
    left.ruleId.localeCompare(right.ruleId)
  );
}

export function runReference(content) {
  const lintResult = lintMarkdown(content, rules, false);
  const fixResult = fixMarkdown(content, { rules });
  const fixedContent = fixResult?.fixedResult?.result ?? content;
  const remainingResult = lintMarkdown(fixedContent, rules, false);

  return {
    diagnostics: sortDiagnostics(normalizeDiagnostics(lintResult)),
    fixedContent,
    changed: fixedContent !== content,
    exitCode: normalizeDiagnostics(remainingResult).length > 0 ? 1 : 0
  };
}
