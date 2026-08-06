# Known compatibility differences

The expanded differential corpus intentionally records observable differences instead of removing the cases or weakening comparisons. These baselines are test expectations, not declarations that the Rust behavior is preferable.

## BOM handling

- `no-full-width-number/bom-prefix`

The reference and Rust implementation disagree on the diagnostic column, fixed output and whether a full-width digit remains after fixing. The case remains in the corpus so either implementation changing its BOM behavior invalidates the baseline.

## Blockquote node boundaries

- `no-multiple-space-blockquote/crlf`
- `no-multiple-space-blockquote/in-list`

The Rust scanner evaluates physical lines whose first non-whitespace character is `>`. The TypeScript reference evaluates parsed blockquote nodes, so it treats consecutive quoted lines and list-contained blockquotes differently.

## Mixed line-ending post-fix exit code

- `no-multiple-blank-lines/mixed-endings`

Both implementations emit the same initial diagnostics and fixed output, but the TypeScript reference still reports a remaining diagnostic after its mixed-ending fix while Rust considers the fixed document clean.

`expected-mismatches.json` stores only the machine-checked category sets. This file preserves the reason each baseline exists.
