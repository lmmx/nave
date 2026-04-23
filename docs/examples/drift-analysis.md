# Drift analysis: dependabot configs

This is the canonical introductory example: use `nave build` to find the shared
skeleton across a set of configs, then act on the drift it surfaces.

## Setup

Assuming `nave init`, `nave scan`, `nave pull` have run:

```bash
nave build --filter dependabot
```

Output on a 9-repo fleet:

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

## Reading the report

- **9 instances** — all dependabot configs across your fleet share the same shape.
- **3 holes** — three positions where they diverge.
- **Cooldown is absent from 6, present in 3, constant when present** — candidate for
  standardisation. Either add it everywhere or drop it everywhere.
- **Intervals split 6/3** — likely a "most repos are weekly, stragglers are monthly"
  situation. Probably worth aligning.

## Acting on it

To find the 3 monthly repos:

```bash
nave search \
  --match 'file:.github/dependabot.yml updates[0].schedule.interval=monthly' \
  --sort pushed-at
```

To create a pen scoped to those repos (for a future codemod that would change their
interval):

```bash
nave pen create \
  --match 'file:.github/dependabot.yml updates[0].schedule.interval=monthly' \
  --name nave/unify-dependabot-intervals \
  monthly
```

The positional `monthly` is a fallback search term; the `--match` predicate is what
actually narrows the selection structurally.

## JSON output for scripting

```bash
nave build --filter dependabot --json > dependabot-drift.json
jq '.groups[].holes[] | select(.distinct_values | length > 1) | .address' \
   dependabot-drift.json
```

Returns every hole with at least two distinct values — i.e. every point of actual
divergence.
