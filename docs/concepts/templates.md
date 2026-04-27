# Templates

A template is the shared skeleton across a set of configs, produced by
simplifying their parsed trees. Where the configs agree, the template
preserves the literal value; where they disagree, the template places a
variable *"hole"* whose values and counts you can inspect.

## Anti-unification

Given a set of terms (here, parsed TOML or YAML values), anti-unification finds the
most specific term that generalises all of them, using fresh variables where they
disagree. For trees, this is structural recursion: walk in parallel, agree → copy the
literal, disagree → emit a hole.

It's thresholdless (no tuning parameters) and has been well-studied since
[Plotkin and Reynolds, 1970](https://webcms3.cse.unsw.edu.au/static/uploads/course/COMP3431/16s2/d16c590ce547157f3d267e60cfc50edae666cde2afae46d90a3848df1232c713/Machine_Intelligence_5_1970_Plotkin.pdf).

See the [Wikipedia article](https://en.wikipedia.org/wiki/Anti-unification) for the
formal treatment.

## Set-based array handling

Arrays of objects (such as CI workflow steps or dependabot update entries) are
anti-unified with **set semantics** rather than positional alignment. Elements
are clustered by structural similarity — specifically, by the intersection of
key-sets across all elements — and anti-unified within each cluster.

This means that a checkout step in position 0 of one workflow and position 2
of another will be recognised as the same kind of step and grouped together.
Within a cluster, variation becomes holes; clusters present in some instances
but not others become optional set elements.

Arrays of scalars (such as Python version lists) continue to use positional
alignment.

Hole addresses for set elements use a predicate label showing the varying
keys: `steps[name,run].run` means "the step cluster whose `name` and `run`
keys vary." In profile display, these are simplified to `steps[]` for
readability.

## What [++"nave"++](../reference/cli/nave.md) does with it

[++"nave build"++](../reference/cli/build.md) groups tracked files by their logical kind (e.g. "all dependabot
configs") and anti-unifies each group. The output, for a fleet of 9 dependabot
configs, looks like this:

```yaml
━━ .github/dependabot.yml ━━
  instances: 9

  template:
    updates:
      - cooldown?: ⟨?0⟩
        directory: "/"
        package-ecosystem: ⟨?1⟩
        schedule:
          interval: ⟨?2⟩
    version: 2

  holes:
    updates[0].cooldown  [optionalkey]  3/9 optional  [constant when present]
        3× {"default-days":7}
    updates[0].package-ecosystem  [string]  9/9
        8× "github-actions"
        1× "cargo"
    updates[0].schedule.interval  [string]  9/9
        6× "weekly"
        3× "monthly"
```

Read as: across 9 files, three hole positions exist. The ecosystem is `github-actions`
in 8 instances and `cargo` in one. The cooldown is absent from 6 files and, when
present, always takes the same value.

## Hole kinds

Holes are classified by how they vary:

- **Scalar** — a leaf differs (the common case).
- **OptionalKey** — a key is present in some files but not others. The hole's presence
  count (e.g. `3/9`) tells you the ratio. When present, Nave descends into the subtree
  rather than treating it as opaque.
- **Shape** — two trees disagree structurally at some position (different types, or
  different array lengths). This is rare for machine-authored configs and usually
  indicates genuine drift worth investigating.

## Source hints

Some holes aren't really variables — they're functionally determined by the repo they
live in. `project.name` in `pyproject.toml` is the clearest case: every file has a
distinct value, but the value *is* the repo name (with the conventional
`kebab-case` ↔ `snake_case` allowance, and PEP 503 normalisation).

After anti-unifying, Nave checks each hole's observed values against per-repo names.
If every observed value matches, the hole is flagged `DerivedFromRepoName` in the
build report:

```
project.name  [string]  40/40  [derived: repo name]
```

This catches `project.name` directly, and separately flags things like
`tool.coverage.run.source[0]` and `tool.isort.known_first_party[0]` — these look like
free parameters, but their values are always the Python module path, which equals the
normalised package name.

The other hint is `ConstantWhenPresent` — for optional keys whose value, when present,
is always the same (like the `cooldown` block above). This is a signal that the key
could be made mandatory or removed entirely without functional impact.

## Cohorts

A cohort is the subset of files sharing the same value at a given hole. "6× weekly /
3× monthly" are two cohorts of the `updates[0].schedule.interval` hole.

Cohorts are how you decide which configs to bulk-edit. "Move the 3 monthly repos to
weekly" is a single codemod targeting a single cohort; you pass the same query to
[++"nave pen create"++](../reference/cli/pen/create.md) that you'd pass to
[++"nave build --match"++](../reference/cli/build.md) to isolate it.

## Profiles

A profile is a **formal concept** from [formal concept analysis][fca] (FCA): a
maximal pair of (repos, hole-value bindings) where every repo in the set
shares every binding, and every binding is shared by every repo.

[fca]: https://en.wikipedia.org/wiki/Formal_concept_analysis

Profiles are computed automatically by `nave build` from the hole
observations. They answer the question "which configuration choices travel
together across my fleet?"

For example, in a fleet of 9 dependabot configs with two holes
(package-ecosystem and schedule interval), FCA discovers three profiles:

- 5 repos use `github-actions` + `weekly`
- 3 repos use `github-actions` + `monthly`
- 1 repo uses `cargo` + `weekly`

These three profiles completely explain the variation in the fleet.

When the concept lattice has hierarchical structure (some profiles are
refinements of others), the display shows deltas: only the bindings that
are new relative to parent profiles. This makes the progressive
specialisation of configuration visible — "Profile 2 refines Profile 1 by
adding the specific version detection script."

## Why it works on configs

Anti-unification gives clean results on dependabot/pyproject/workflows because those
files are *structurally rigid*: they're machine-readable, share a spec, and humans
rarely reshape them. On freeform prose it would be useless; on config it recovers
exactly the template you'd write by hand.

## Co-occurrence mode

Whole-file anti-unification can be too coarse. If you have 20 workflow files, each
containing 5 steps, and you only care about the `uses:` pattern for a specific
action, template-ing the whole file drowns the signal.

[++"nave build --co-occur --where ... --where ..."++](../reference/cli/build.md)
addresses this by anti-unifying *subtrees*
where multiple `--where` terms co-occur, rather than whole files.
The rule can be considered as
"the deepest non-root object ancestor shared by all anchor matches".
