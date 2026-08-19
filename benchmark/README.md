# The bench

Three levels and a stopwatch. Everything here runs against the Rust
implementation alone: the Python it replaced is no longer in the tree, so the
question is no longer "do the two agree?" but "does this one still compute what
it computed, reproducibly, and correctly?"

Run it in release. A debug build measures the debug build.

```bash
cargo run --release -p mosna-bench -- all
```

## Why three levels

MOSNA is not uniformly deterministic, and a bench that pretended otherwise
would either fail at random or prove nothing. Each level asks the strongest
question its subject can answer.

| Level | Subject | Question | Command |
|---|---|---|---|
| 1 — golden | geometry, NAS, mixing matrices, assortativity | are the numbers the ones recorded in `golden/`? | `golden` |
| 2 — reproducibility | permutation null, UMAP, GMM, Leiden | does the answer depend on how many threads ran? | `reproduce` |
| 3 — recovery | the pipeline end to end | do the niches found match the niches planted? | `recover` |

Level 1 catches numerical drift — a refactor that moves the fifteenth digit,
which nobody notices until results move. Level 2 catches a result that depends
on scheduling. Neither would notice an analysis that is reproducibly wrong,
which is what level 3 is for: the tissue is generated with known niches, so
there is a ground truth to score against.

## The data

`cohort.rs` generates tissue rather than shipping it. Each sample has niche
centres scattered over a field; each niche has its own lopsided phenotype
mixture; a cell picks a niche, lands near its centre, and takes a phenotype
from that niche's mixture.

That is deliberately the model the niche analysis assumes. Uniform noise would
leave every partition as good as every other, and level 3 would measure
nothing.

Every draw descends from one seed, so a cohort is reproducible — which is what
lets a golden reference exist at all. Changing `--seed` changes the data, and
therefore the reference: the seed and the cohort shape are both in the
reference's file name so the two can never be silently compared.

## What "the same" means, level by level

**Labels are exact.** A cell either landed in the same niche or it did not.
`gmm_labels` and `leiden_labels` are compared bit for bit.

**Edges are exact but unordered.** A parallel pipeline returns them in whatever
order the threads finished in, so they are sorted before hashing. Order is not
a result.

**Numbers get a tolerance of 1e-12 relative.** A parallel reduction associates
its additions differently depending on how the work was split — `(a+b)+c`
against `a+(b+c)` — and floating-point addition is not associative. The
permutations themselves are identical, which is the part that matters; the
residual is around 1e-15.

**`NaN` equals `NaN`.** A grey cell of a mixing matrix — a pair of phenotypes
that never occur together — is absent in both runs, and that is agreement.

## Recording a new reference

```bash
cargo run --release -p mosna-bench -- golden --update
```

Then read the diff of `golden/`. It names every stage that moved, which is the
point of committing these files: an intended change is visible and an
accidental one is loud.

## What the bench found

In short: level 2 caught Leiden
breaking ties by `HashMap` iteration order, which is seeded per thread. The
same graph was partitioned differently on a worker thread than on the main
one — and differently again in the next process — despite Leiden taking a
seed. Fixed by ordering the maps; two regression tests in
`crates/mosna-core/src/clustering/leiden.rs` now pin it.

## Measured on this machine

Two samples, 2026-08-04, release build:

| Stage | Cells | Median | Per cell | Peak RSS |
|---|---:|---:|---:|---:|
| geometry + NAS + assortativity | 4 000 | 6.1 ms | 2 µs | 4.7 MB |
| geometry + NAS + assortativity | 20 000 | 24.9 ms | 1 µs | 10.1 MB |
| geometry + NAS + assortativity | 100 000 | 198.4 ms | 2 µs | 35.0 MB |
| UMAP + GMM | 2 000 | 697.5 ms | 349 µs | 6.1 MB |
| UMAP + GMM | 10 000 | 3.85 s | 385 µs | 17.9 MB |
| UMAP + GMM | 50 000 | 12.80 s | 256 µs | 101.4 MB |

The per-cell column is the one to read: flat across a twenty-five-fold increase
in size, so both halves scale linearly and nothing quadratic is hiding.

Level 3 on 3 000 cells recovers the planted niches at an adjusted Rand index of
0.51 with the right number of niches found — well above chance, and short of
perfect because a cell on a boundary genuinely belongs to both niches.

## Note on peak memory

It is the high-water mark of the whole process, so across a sweep it only ever
grows. Read the largest row, or run one size at a time.
