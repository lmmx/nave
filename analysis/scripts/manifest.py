"""Write a TSV manifest of a nave cache: repo, tree_sha, path, blob_sha (pins the analysed inputs)."""

from __future__ import annotations

import sys
from pathlib import Path

import tomllib

root = Path(sys.argv[1]) / "fleet" / "lmmx"
for d in sorted(root.iterdir()):
    meta = tomllib.loads((d / "meta.toml").read_text())
    for path, sha in sorted(
        tomllib.loads((d / "tracked.toml").read_text())["files"].items()
    ):
        print(f"{d.name}\t{meta.get('tree_sha')}\t{path}\t{sha}")
