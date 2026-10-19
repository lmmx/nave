# analysis/

Scripts, inputs and commands behind [`../FINDINGS.md`](../FINDINGS.md).

- Revision analysed: `5a06c97` (`master`), Rust 1.95 (`rust-toolchain.toml`), Python 3.13.
- Python deps (pinned for the reproduction): `numpy==2.3.4 scipy==1.16.3 pyyaml==6.0.3`.
- Run `uv` with `--no-project` (or from outside the repo): plain `uv run` inside the repo rewrites `uv.lock`.

## Layout

| Path | What |
|---|---|
| `scripts/populate.py` | Stands in for `nave scan` (the session's GitHub API was limited to this repo): blobless depth-1 clone of each repo, writes `meta.toml`/`tracked.toml` in nave's cache layout. |
| `scripts/manifest.py` | Prints repo / tree SHA / path / blob SHA for a cache, to pin inputs. |
| `scripts/analyse.py` | Key-path presence spectrum, Jaccard + average-linkage clustering, silhouette, creation-date Mantel test. |
| `scripts/analyse2.py` | Same with "dictionary" maps collapsed to `*` (hand-written regex list `DICT`); FCA attribute budget from `au_*.json`. |
| `scripts/analyse3.py` | Describes the dictionary-aware clusters (workflows k=6, Cargo k=2). |
| `scripts/crosstab.py` | Workflow clusters (k=6) × filename. |
| `scripts/pyproject_inventory.py` | Every `pyproject.toml` at any depth; whether nave indexes it / has it on disk. |
| `scripts/polars_groups.py` | Polars dependency groups, with dates from git history. |
| `patches/fca-experiment.patch` | Env-gated instrumentation of `crates/nave_build/src/fca.rs` (not merged). Flags: `NAVE_NO_FCA`, `NAVE_FCA_CAP`, `NAVE_FCA_COUNT_ONLY`, `NAVE_FCA_MIN_SUPPORT`, `NAVE_FCA_NO_ABSENT`. All unset = stock behaviour plus one `EXPERIMENT ...` line on stderr. |
| `exp/page*.json` | Snapshot of `repos.ecosyste.ms` metadata for `lmmx` (394 repos), trimmed to the fields used. Fetched 2026-10-17. |
| `exp/manifest-original.tsv` | Inputs of the first run (140 repos, 392 files). |
| `exp/out-*`, `exp/*.tsv`, `exp/*.txt` | Outputs quoted in FINDINGS.md. |

## Commands

The snapshot was fetched with:

```bash
for p in 1 2 3 4 5 6 7 8; do
  curl -sS "https://repos.ecosyste.ms/api/v1/hosts/GitHub/owners/lmmx/repositories?per_page=100&page=$p" -o page$p.json
done   # pages 5-8 were empty; then trimmed into exp/
```

Reproduction (fresh `HOME`, from the repo root):

```bash
git apply analysis/patches/fca-experiment.patch && cargo build --release && git checkout crates/nave_build/src/fca.rs
N=$PWD/target/release/nave
R=/path/to/fresh-home; mkdir -p $R && cp ~/.gitconfig $R/
PY="uv run -q --no-project --with numpy==2.3.4 --with scipy==1.16.3 --with pyyaml==6.0.3 python -I"

# 1. cache
HOME=$R NAVE_GITHUB__USERNAME=lmmx $N init --no-interaction
HOME=$R python3 -I analysis/scripts/populate.py analysis/exp
printf 'username = "lmmx"\n' > $R/.cache/nave/meta.toml
HOME=$R NAVE_GITHUB__USERNAME=lmmx $N pull
python3 -I analysis/scripts/manifest.py $R/.cache/nave | diff - analysis/exp/manifest-original.tsv   # empty on 2026-10-17

# 2. FCA baseline and ablation (pre-commit)
HOME=$R NAVE_GITHUB__USERNAME=lmmx NAVE_FCA_COUNT_ONLY=1                      $N build --filter pre-commit --json >/dev/null
HOME=$R NAVE_GITHUB__USERNAME=lmmx NAVE_FCA_COUNT_ONLY=1 NAVE_FCA_NO_ABSENT=1 $N build --filter pre-commit --json >/dev/null

# 3. anti-unification only (inputs to analyse2.py)
for f in dependabot pre-commit pyproject Cargo workflows; do
  HOME=$R NAVE_GITHUB__USERNAME=lmmx NAVE_NO_FCA=1 $N build --filter $f --json > analysis/exp/au_$f.json
done

# 4. analyses
for s in analyse analyse2 analyse3 crosstab; do HOME=$R $PY analysis/scripts/$s.py > analysis/exp/out-$s.txt; done
python3 -I analysis/scripts/pyproject_inventory.py analysis/exp $R/.cache/nave $R/extra-pyprojects > analysis/exp/pyproject-inventory.tsv
python3 -I analysis/scripts/polars_groups.py $R/.cache/nave $R/extra-pyprojects analysis/exp/pyproject-inventory.tsv > analysis/exp/out-polars-groups.tsv
```

Earlier runs in the same session (FCA cap and min-support sweeps) used the same binary with
`NAVE_FCA_CAP=1000|20000|200000` and `NAVE_FCA_MIN_SUPPORT=5|10`, under `timeout 120`.

## Caveats on inputs

- Repos are cloned live, so a rerun after new pushes will differ; diff against `exp/manifest-original.tsv`.
- ecosyste.ms metadata can be stale (`last_synced_at` goes back to 2023 for some repos); `fork`, `archived`
  and `size` come from it, not from GitHub.
- `scripts/populate.py` matches `tracked_paths` with `fnmatch` on the lowercased path. This agrees with nave's
  `PathMatcher` (globset, anchored, `*` crosses `/`) on the probes checked: `pyproject.toml` root-only,
  `.github/workflows/*.yml` matching nested paths.
