# Findings: `nave build` on the lmmx fleet

Revision `5a06c97`. 140 repos / 392 tracked files (pinned in `analysis/exp/manifest-original.tsv`), rebuilt
from scratch on 2026-10-17 with identical inputs and byte-identical outputs. Method, commands, caveats: `analysis/README.md`.
Of 243 non-fork, non-archived repos: 99 have no tracked file, 3 are empty per ecosyste.ms, 1 (`ntnts`) is not publicly clonable.

## What was measured

- **FCA, pre-commit** (101 files, 53 holes). Stock: 236 attributes (183 value + 53 "key absent") → **44,643 concepts**,
  enumerated in 0.2 s. Ablating only the key-absent attributes: 183 → **109 concepts**. Dropping values seen in < 5 / < 10
  files instead: 44,597 / 44,557. The > 120 s runtimes are spent after enumeration (profile and lattice construction).
- **FCA, other groups** (cap/min-support runs, no ablation): pyproject and Cargo > 200k concepts; workflows 2,544,730 at
  min-support 5; unfiltered `nave build` was killed out of memory.
- **Templates.** pyproject: 97 files, 335 key paths, none in every file, 197 in exactly one; template is 294 lines.
- **Dictionary-aware key paths** (open-keyed maps collapsed to `*`, hand-written list): workflows 959 → 122 paths, best
  silhouette 0.17 → 0.57; Cargo 670 → 131, 0.19 → 0.40; pyproject 335 → 186, 0.32 → 0.40.
- **Workflow clusters (k=6) × filename** (`exp/out-crosstab.txt`): filename predicts cluster for 94% of 134 files (largest
  cluster alone: 34%). `CI.yml` splits 38 / 2 / 3 across the publish, test and binaries clusters; `pages.yml` and
  `publish_to_pypi.yml` share the publish cluster. `deploy.yml` (27) is a manual "Deploy Release" running `just release`.
- **Creation-date Mantel test**, pyproject: Spearman ρ = 0.35 between config distance and |Δ repo creation date|; 0 of 200
  permutations reached it (max 0.168). pre-commit ρ = −0.03.
- **pyproject coverage.** nave's `pyproject.toml` pattern is root-only (probed against `PathMatcher`). 44 subdirectory
  `pyproject.toml` files in 16 repos are not indexed; 30 of them are on disk anyway, because the sparse-checkout pattern is unanchored.
- **Polars groups** (name contains "polars", or every requirement is a polars distribution; 34 definitions, 6 mixed groups
  excluded; per-file last-changed dates in `exp/out-polars-groups.tsv`):

  | Form (versions → `<v>`) | Files | Current form since |
  |---|---|---|
  | `optional-dependencies` `polars = ["polars>=<v>"]` | 12 (2 subdir) | 2025-02-06 – 2025-03-13; polars-genson 2026-09-26 |
  | `optional-dependencies` `polars-lts-cpu = ["polars-lts-cpu>=<v>"]` | 9 (1 subdir) | 2025-02-06 – 2025-03-13 |
  | `dependency-groups` `polars = ["polars>=<v>"]` | 2 | 2025-12-06 |
  | `optional-dependencies` `polars-lts-cpu = ["polars[rtcompat]"]` | 1 (subdir, polars-fastembed) | 2025-12-14 |
  | `optional-dependencies` `lts-cpu = ["polars[rtcompat]>=<v>"]` | 7 | 2026-04-14 – 04-16 |
  | `dependency-groups` `lts-cpu = ["polars[rtcompat]>=<v>"]` | 2 | 2026-04-15 – 04-16 |
  | `optional-dependencies` `rtcompat = ["polars[rtcompat]>=<v>"]` | 1 (subdir, polars-genson) | 2026-09-26 |

  datapasta, polars-expr-hopper and polars-permute-plugin were last changed 14–16 Apr 2026 and still carry
  `polars-lts-cpu = ["polars-lts-cpu>=…"]`. PyPI: `polars-lts-cpu` last released 2025-09-23 (1.34.0b1; stable 1.33.1);
  `polars-runtime-compat` first released 2025-09-26, latest 2.0.0 on 2026-10-06.

## What is claimed

- In pre-commit, the key-absent attributes, not rare values, cause the concept explosion (410× reduction when removed).
- Two earlier statements were wrong: FCA enumeration itself is fast on pre-commit, and `deploy.yml` is not a Pages deploy.
- With dictionary-aware paths, workflow clusters mostly rediscover the filename conventions. Beyond filenames they show
  only that `CI.yml` covers three roles, and that Pages and PyPI publishing share `environment` + `id-token`.
  Grouping by filename would get most of the benefit.
- pyproject distance tracks repo age beyond chance (p ≤ 1/201).
- Anything that reads nave's index misses subdirectory packages, including 3 of the 18 repos with polars groups.
- `polars[rtcompat]` is the current way to get the compatibility runtime. `polars-lts-cpu` groups depend on a package
  with no release since September 2025.

## What is conjecture

- Key-absent attributes also drive the blow-up in pyproject, Cargo and workflows (ablation run on pre-commit only).
- pyproject drifts along an era gradient (copied templates) rather than forming discrete groups. Creation date is only a
  proxy for config age, and the workflow Mantel test permutes files rather than repos, so its p-value is unreliable.
- `lts-cpu = ["polars[rtcompat]>=…"]` is the fleet convention (rolled out 14–16 Apr 2026, three repos missed).
  `rtcompat = [...]` (polars-genson, Sep 2026) may be where the naming is heading, but it is a single file.
- Reading `additionalProperties`/`patternProperties` from the cached JSON Schemas would reproduce the hand-written dictionary list.
