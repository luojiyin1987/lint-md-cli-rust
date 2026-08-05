# Validation plan

The prototype should answer whether a native implementation is worth further investment. It is not enough to show that Rust can lint a file.

## Questions

1. Does a native binary materially improve cold-start latency for pre-commit and editor workflows?
2. Does it reduce peak memory on large file sets?
3. How much behavior differs once Unicode, code spans, fenced blocks, CRLF and fixes are included?
4. Is the maintenance cost of a second implementation justified by measurable user value?

## Measurements

Build a release binary first:

```bash
cargo build --release
```

Suggested measurements:

```bash
hyperfine --warmup 3 \
  'target/release/lint-md-rs README.md' \
  'npx lint-md README.md'
```

Record at least:

- cold-start latency;
- warm latency;
- peak resident memory;
- throughput over many small files;
- throughput and peak memory for 1 MiB and 5 MiB files;
- release binary size;
- mismatches against the TypeScript reference.

## Compatibility corpus

The differential test corpus should include:

- LF and CRLF;
- UTF-8 BOM;
- Chinese punctuation and full-width digits;
- emoji and combining characters;
- inline code with different backtick widths;
- backtick and tilde fences;
- empty and whitespace-only input;
- symlinks and write failures;
- fixes that overlap or require multiple passes.

## Continue / stop criteria

Continue toward an AST-backed native engine only when all of the following are true:

- cold-start or memory improvement is meaningful for real workflows;
- the compatibility harness is automated;
- mismatch categories are understood rather than hidden;
- the project can keep the TypeScript implementation as an oracle during migration.

Stop or keep the project as a research prototype when the measurable gains are small compared with the cost of parser and rule compatibility.
