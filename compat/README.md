# TypeScript–Rust compatibility harness

This directory runs the TypeScript `@lint-md/core` implementation and the Rust prototype over the same fixtures, normalizes their observable results, and rejects unrecorded compatibility differences.

## Prepare the pinned TypeScript reference

Run from the repository root:

```bash
gh repo clone luojiyin1987/lint-md compat/core-reference
git -C compat/core-reference checkout 9c88a15d43a9dd8637c57157774214a379807d26
npm install --prefix compat/core-reference --ignore-scripts --no-audit --no-fund
npm run build --prefix compat/core-reference
```

Then build Rust and run the comparison:

```bash
cargo build
npm test --prefix compat
```

The runner defaults to `compat/core-reference/lib/index.js`. Set `LINT_MD_CORE_REFERENCE` to use another compatible build.

Known, intentional prototype differences are recorded in `expected-mismatches.json`. New mismatch categories fail the run; resolved baseline entries also fail so the baseline cannot silently become stale.
