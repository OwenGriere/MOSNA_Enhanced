# Testing discipline

The port is written test-first. This file states the rule, so a later session —
or a later contributor — applies the same one.

## The rule

**No implementation is written before the tests that pin it, and no module is
considered done until its tests are green.**

For each Python function being ported:

1. Read the Python and write down what it must do — including the behaviours
   that look like bugs, which are reproduced deliberately and marked as such.
2. Write the tests. Example-based tests for the known answers, property tests
   for the invariants that must hold on every input.
3. Run them. They must fail, and fail for the expected reason. A test that
   passes against an empty implementation is testing nothing.
4. Implement until green.
5. Run `cargo fmt`, `cargo clippy` and the full suite before moving on.

Steps 3 and 4 are what caught the two defects recorded in `PROGRESS.log`: the
matrix-versus-elementwise product in `attribute_ac`, and the ambiguous file-name
decoding in `find_sample_from_file`. Neither would have been visible by reading
the Python and transcribing it.

## Using an existing crate

Reaching for a published crate instead of implementing an algorithm is fine and
often right. The bar is the same either way: **the crate has to pass the tests
that were written first.** If it does not, either it is the wrong crate or the
test states a requirement the crate does not meet — and that has to be resolved
before the crate goes in, not after.

Crates already vetted this way:

| Crate | Used for | Why not hand-written |
|---|---|---|
| `delaunator` | Delaunay triangulation | Robust geometric predicates; matches `scipy.spatial.Voronoi(...).ridge_points` |
| `kiddo` | k-d tree for k-nearest neighbours | Exact queries, no approximation drift |
| `parquet` / `arrow-*` | Table I/O | The reference implementation of the format |

Deliberately hand-written, with the reason:

| Algorithm | Why |
|---|---|
| Symmetric eigensolver, Cholesky, k-means | Avoids a LAPACK/BLAS build dependency, which is fragile on Windows; the matrices are small |
| Mixing matrix, NAS aggregation | The hot loops; a generic library would not exploit the one-hot structure |
| Ward linkage | Needs scipy's exact linkage-matrix conventions |

## Kinds of test

**Example-based** (`#[test]` in a `mod tests` inside the module): known inputs
with known answers, plus every degenerate case — empty input, a single point,
collinear points, an all-null column.

**Property-based** (`proptest!` in `tests/prop_*.rs`): invariants over generated
inputs. These are the ones that find what nobody thought to write down. The
naming convention matters: the CI runs `cargo test --workspace prop_` with a
raised case budget, so a property test must be named `prop_*`.

**Integration** (`tests/real_*.rs`): run against the real files checked into the
repository — `CONFIG/configuration.yaml`, `test/patient_folder`,
`test/patient_sample_folder`. These catch the mismatches synthetic fixtures
cannot.

Shared fixtures, generators and assertions live in `crates/mosna-testkit`.

## Running

```bash
cargo test --workspace                  # everything, default budget
cargo test --workspace prop_            # property tests only
PROPTEST_CASES=2048 cargo test --workspace prop_   # what CI runs
cargo fmt --all --check
cargo clippy --workspace --all-targets  # CI adds -D warnings
```

When a property test fails, proptest writes the failing seed to
`proptest-regressions/` next to the test. That file is committed: it turns a
one-off discovery into a permanent regression test.

## CI

`.github/workflows/rust.yml` runs on every push and pull request:

- **test** — formatting, clippy with `-D warnings`, build, unit and integration
  tests, doc tests. On Linux and Windows, because the application ships on both.
- **property tests (deep)** — the `prop_*` tests with `PROPTEST_CASES=2048`,
  roughly eight times the local budget.
- **rustdoc** — the documentation builds without warnings.
