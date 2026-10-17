"""List every pyproject.toml (any depth) in the same repo set populate.py uses, and whether nave indexes it.

Usage: python -I pyproject_inventory.py <exp_dir> <cache_root> <extra_dir>
Writes TSV to stdout: repo, path, blob_sha, in_nave_index (listed in tracked.toml), on_disk (present in nave's checkout).
Files not in nave's index are saved under <extra_dir>/<repo>/<path>.
"""
import json, glob, subprocess, sys, tempfile, shutil, tomllib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

exp, cache, extra = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
repos = [r for f in sorted(glob.glob(str(exp / "page*.json"))) for r in json.load(open(f))]
repos = [r for r in repos if not r["fork"] and not r["archived"] and (r.get("size") or 0) > 0]

def one(r):
    name = r["full_name"].split("/")[1]
    tmp = tempfile.mkdtemp()
    try:
        subprocess.run(["git", "clone", "-q", "--depth", "1", "--filter=blob:none", "--no-checkout",
                        f"https://github.com/lmmx/{name}.git", tmp], check=True, capture_output=True, timeout=120)
        out = subprocess.run(["git", "-C", tmp, "ls-tree", "-r", "HEAD"], capture_output=True, text=True, check=True).stdout
        rows = []
        for line in out.splitlines():
            meta, path = line.split("\t", 1)
            _, typ, sha = meta.split()
            if typ == "blob" and Path(path).name.lower() == "pyproject.toml":
                rd = cache / "fleet" / "lmmx" / name
                indexed = (rd / "tracked.toml").exists() and path in tomllib.loads((rd / "tracked.toml").read_text())["files"]
                on_disk = (rd / "checkout" / path).exists()
                if not indexed:
                    dest = extra / name / path
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    dest.write_bytes(subprocess.run(["git", "-C", tmp, "cat-file", "-p", sha], capture_output=True, check=True).stdout)
                rows.append((name, path, sha, indexed, on_disk))
        return rows
    except Exception as e:
        return [(name, f"ERROR {type(e).__name__}", "", False, False)]
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

with ThreadPoolExecutor(16) as ex:
    for rows in ex.map(one, repos):
        for name, path, sha, indexed, on_disk in rows:
            print(f"{name}\t{path}\t{sha}\t{indexed}\t{on_disk}")
