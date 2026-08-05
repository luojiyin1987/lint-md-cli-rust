# lint-md-cli-rust

A Rust validation prototype for a possible native [`lint-md`](https://github.com/lint-md/cli) command-line interface.

This repository is an experiment, not a drop-in replacement. The TypeScript CLI remains the compatibility reference.

## What this prototype validates

- Native single-binary startup and distribution.
- A stable diagnostic model: rule ID, line, column, severity and fixability.
- Deterministic safe fixes while preserving LF/CRLF line endings.
- Text output for humans and JSON output for differential tests.
- Cross-platform behavior on Linux, macOS and Windows.
- A hybrid rule engine that keeps text rules lightweight while using mdast source positions where Markdown syntax matters.

The implementation uses [`markdown-rs`](https://github.com/wooorm/markdown-rs) for CommonMark-compatible inline-code parsing. Scanner-only rules remain lightweight so parser cost and compatibility gains can be measured independently.

## Supported input

The prototype accepts one UTF-8 Markdown file or standard input:

```bash
cargo run -- README.md
printf '版本１２\n' | cargo run -- --stdin
```

Apply fixes:

```bash
cargo run -- --fix README.md
printf '版本１２\n' | cargo run -- --stdin --fix
```

Emit JSON:

```bash
cargo run -- --format json README.md
```

Exit codes:

| Code | Meaning |
| ---: | --- |
| 0 | Clean, or all findings were safely fixed |
| 1 | Findings remain |
| 2 | Usage or I/O failure |

## Prototype rules

| Rule | Fix | Notes |
| --- | --- | --- |
| `no-full-width-number` | Yes | Groups contiguous digits and skips inline and fenced code |
| `no-empty-inline-code` | Yes | Uses mdast values and source ranges, including whitespace-only code spans |
| `no-empty-blockquote` | Yes | Removes empty blockquotes |
| `no-multiple-space-blockquote` | Yes | Inserts or collapses spacing after the first `>` marker |
| `no-multiple-blank-lines` | Yes | Normalizes leading, trailing and repeated blank lines while preserving line endings |

The engine is intentionally hybrid. Rules that depend on Markdown node semantics use mdast positions; document-level and text-oriented rules continue to use focused scanners. Fixes are repeated until stable so interactions between rules converge on the same output as the TypeScript reference.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Validation plan

See [`docs/VALIDATION.md`](docs/VALIDATION.md). Compatibility is continuously checked by running the pinned TypeScript Core and this prototype over the same fixture corpus, then comparing diagnostics, exit codes and fixed output.

## Non-goals

- Reimplementing the complete `@lint-md/core` rule set.
- Replacing the TypeScript CLI before compatibility data exists.
- Moving scanner rules to an AST without a demonstrated semantic need.
