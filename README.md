# lint-md-cli-rust

A dependency-free Rust validation prototype for a possible native [`lint-md`](https://github.com/lint-md/cli) command-line interface.

This repository is an experiment, not a drop-in replacement. The TypeScript CLI remains the compatibility reference.

## What this prototype validates

- Native single-binary startup and distribution.
- A stable diagnostic model: rule ID, line, column, severity and fixability.
- Deterministic safe fixes while preserving LF/CRLF line endings.
- Text output for humans and JSON output for differential tests.
- Cross-platform behavior on Linux, macOS and Windows.

The implementation intentionally uses only the Rust standard library. This makes startup, binary size and supply-chain measurements easier to interpret.

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
| `no-full-width-number` | Yes | Skips inline code and fenced code |
| `no-empty-inline-code` | Yes | Removes exact unescaped empty backtick pairs |
| `no-empty-blockquote` | No | Demonstrates a non-fixable diagnostic |
| `no-multiple-space-blockquote` | Yes | Collapses spaces after the first `>` marker |
| `no-multiple-blank-lines` | Yes | Keeps one blank line and preserves line endings |

These are line-oriented approximations. They deliberately do not claim full Markdown AST compatibility.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Validation plan

See [`docs/VALIDATION.md`](docs/VALIDATION.md). The next useful step is a differential harness that runs the TypeScript CLI and this prototype over the same fixture corpus, then compares diagnostics, exit codes and fixed output.

## Non-goals

- Reimplementing the complete `@lint-md/core` rule set.
- Replacing the TypeScript CLI before compatibility data exists.
- Treating regex or line scanning as a sufficient long-term Markdown parser.
