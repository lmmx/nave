"""Stand in for `nave scan`: list each repo's tree via a blobless clone and write nave's cache files."""

from __future__ import annotations

import fnmatch
import glob
import json
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

exp = Path(sys.argv[1])
cache = Path.home() / ".cache/nave"
PATS = [
    "pyproject.toml",
    "cargo.toml",
    ".pre-commit-config.yaml",
    ".pre-commit-config.yml",
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
    ".github/dependabot.yml",
    ".github/dependabot.yaml",
]
repos = [
    r for f in sorted(glob.glob(str(exp / "page*.json"))) for r in json.load(open(f))
]
repos = [
    r for r in repos if not r["fork"] and not r["archived"] and (r.get("size") or 0) > 0
]


def one(r):
    name = r["full_name"].split("/")[1]
    url = f"https://github.com/lmmx/{name}.git"
    tmp = tempfile.mkdtemp()
    try:
        subprocess.run(
            [
                "git",
                "clone",
                "-q",
                "--depth",
                "1",
                "--filter=blob:none",
                "--no-checkout",
                url,
                tmp,
            ],
            check=True,
            capture_output=True,
            timeout=120,
        )
        tree = subprocess.run(
            ["git", "-C", tmp, "rev-parse", "HEAD^{tree}"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        branch = subprocess.run(
            ["git", "-C", tmp, "rev-parse", "--abbrev-ref", "HEAD"],
            capture_output=True,
            text=True,
        ).stdout.strip()
        out = subprocess.run(
            ["git", "-C", tmp, "ls-tree", "-r", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except Exception as e:
        return name, f"ERR {e}"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    files = {}
    for line in out.splitlines():
        meta, path = line.split("\t", 1)
        mode, typ, sha = meta.split()
        if typ == "blob" and any(fnmatch.fnmatchcase(path.lower(), p) for p in PATS):
            files[path] = sha
    if not files:
        return name, 0
    d = cache / "fleet" / "lmmx" / name
    d.mkdir(parents=True, exist_ok=True)
    pushed = r["pushed_at"].replace(".000Z", "Z") if r.get("pushed_at") else None
    (d / "meta.toml").write_text(
        f'owner = "lmmx"\nname = "{name}"\ndefault_branch = "{branch}"\n'
        f'clone_url = "{url}"\ntree_sha = "{tree}"\n'
        + (f'pushed_at = "{pushed}"\n' if pushed else "")
    )
    (d / "tracked.toml").write_text(
        "[files]\n" + "".join(f'"{k}" = "{v}"\n' for k, v in sorted(files.items()))
    )
    return name, len(files)


with ThreadPoolExecutor(16) as ex:
    res = list(ex.map(one, repos))
errs = [x for x in res if isinstance(x[1], str)]
hit = [x for x in res if isinstance(x[1], int) and x[1] > 0]
print(
    f"repos={len(repos)} with_tracked={len(hit)} files={sum(n for _, n in hit)} errors={len(errs)}"
)
for e in errs[:5]:
    print(e)
