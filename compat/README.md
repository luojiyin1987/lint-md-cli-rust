# TypeScript–Rust compatibility harness

This directory runs the TypeScript `@lint-md/core` implementation and the Rust prototype over the same fixtures, normalizes their observable results, and rejects unrecorded compatibility differences.

Run from the repository root:

```bash
cargo build
npm install --prefix compat
npm test --prefix compat
```

Known, intentional prototype differences are recorded in `expected-mismatches.json`. New mismatch categories fail the run; resolved baseline entries also fail so the baseline cannot silently become stale.
