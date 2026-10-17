"""Find pyproject.toml dependency groups/extras that are polars groups, and date them from git history.

A group counts as a "polars group" if its name contains "polars", or every requirement in it is a polars
distribution (polars, polars-lts-cpu, polars-u64-idx, polars-runtime-*), e.g. lts-cpu = ["polars[rtcompat]>=1.30"].
Groups that merely include polars among other packages are reported separately as "mixed".

Usage: python -I polars_groups.py <cache_root> <extra_dir> <inventory.tsv>
"""
import re, subprocess, sys, tempfile, shutil, tomllib, collections
from pathlib import Path

cache, extra, inv = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
POLARS = re.compile(r"^polars(-lts-cpu|-u64-idx|-runtime-[a-z0-9]+)?$")

def req_name(req):
    m = re.match(r"\s*([A-Za-z0-9][A-Za-z0-9._-]*)", req)
    return m.group(1).lower().replace("_", "-") if m else ""

def groups(doc):
    """Yield (table, group_name, [requirement strings]) for every named dependency group/extra."""
    proj = doc.get("project", {})
    for g, reqs in proj.get("optional-dependencies", {}).items():
        yield "project.optional-dependencies", g, list(reqs)
    for g, reqs in doc.get("dependency-groups", {}).items():
        yield "dependency-groups", g, [r for r in reqs if isinstance(r, str)]
    tool = doc.get("tool", {})
    for g, reqs in tool.get("pdm", {}).get("dev-dependencies", {}).items():
        yield "tool.pdm.dev-dependencies", g, list(reqs)
    for g, spec in tool.get("poetry", {}).get("group", {}).items():
        yield "tool.poetry.group", g, [f"{k}{'' if isinstance(v, dict) else v}" for k, v in spec.get("dependencies", {}).items()]
    for g, reqs in tool.get("poetry", {}).get("extras", {}).items():
        yield "tool.poetry.extras", g, list(reqs)

def norm(req):
    return re.sub(r"\d+(\.\d+)*", "<v>", req.replace(" ", ""))

def classify(name, reqs):
    names = [req_name(r) for r in reqs]
    if "polars" in name.lower() or (names and all(POLARS.match(n) for n in names)):
        return "polars"
    if any(POLARS.match(n) for n in names):
        return "mixed"
    return None

def signature(table, name, reqs):
    return f"[{table}] {name} = {[norm(r) for r in reqs]}"

files = []
for line in inv.read_text().splitlines():
    repo, path, sha, indexed, on_disk = line.split("\t")
    if path.startswith("ERROR"):
        continue
    src = (cache / "fleet" / "lmmx" / repo / "checkout" / path) if indexed == "True" else (extra / repo / path)
    files.append((repo, path, indexed == "True", src))

hits, mixed = [], []
for repo, path, indexed, src in files:
    doc = tomllib.loads(src.read_text())
    for table, name, reqs in groups(doc):
        c = classify(name, reqs)
        if c == "polars":
            hits.append((repo, path, indexed, table, name, reqs))
        elif c == "mixed":
            mixed.append((repo, path, table, name, reqs))

def history(repo, paths):
    """Per path: last-change date, and when the current form of each polars group first appeared."""
    tmp = tempfile.mkdtemp()
    try:
        subprocess.run(["git", "clone", "-q", "--filter=blob:none", "--no-checkout",
                        f"https://github.com/lmmx/{repo}.git", tmp], check=True, capture_output=True, timeout=300)
        out = {}
        for path in paths:
            last = subprocess.run(["git", "-C", tmp, "log", "-1", "--format=%cI", "--", path],
                                  capture_output=True, text=True).stdout.strip()
            revs = subprocess.run(["git", "-C", tmp, "log", "--reverse", "--format=%H %cI", "--", path],
                                  capture_output=True, text=True).stdout.split("\n")
            forms = []  # (date, {(table, name): signature})
            for rev in filter(None, revs):
                h, d = rev.split()
                blob = subprocess.run(["git", "-C", tmp, "show", f"{h}:{path}"], capture_output=True, text=True)
                try:
                    doc = tomllib.loads(blob.stdout) if blob.returncode == 0 else {}
                except tomllib.TOMLDecodeError:
                    doc = {}
                forms.append((d, {(t, n): signature(t, n, r) for t, n, r in groups(doc) if classify(n, r) == "polars"}))
            out[path] = (last, forms)
        return out
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

by_repo = collections.defaultdict(set)
for repo, path, *_ in hits:
    by_repo[repo].add(path)
hist = {repo: history(repo, sorted(ps)) for repo, ps in sorted(by_repo.items())}

print("repo\tpath\tindexed\tsignature\tfile_last_changed\tcurrent_form_since\tgroup_first_seen")
rows = []
for repo, path, indexed, table, name, reqs in hits:
    sig = signature(table, name, reqs)
    last, forms = hist[repo][path]
    since = first = ""
    for d, f in forms:
        if (table, name) in f:
            first = first or d
            since = d if f[(table, name)] == sig and (not since or prev != sig) else since
        prev = f.get((table, name))
    rows.append((sig, repo, path, indexed, last, since, first))
for r in sorted(rows):
    print("\t".join([r[1], r[2], str(r[3]), r[0], r[4][:10], r[5][:10], r[6][:10]]))
print("\n# mixed groups (polars alongside other packages; not counted):")
for repo, path, table, name, reqs in mixed:
    print(f"# {repo}\t{path}\t[{table}] {name} = {reqs}")
