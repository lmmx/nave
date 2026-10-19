from __future__ import annotations

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import analyse as A  # reuse loaders (runs analyse main too, suppressed below)

# Maps whose keys are data (open key sets), approximating what JSON Schema marks as
# additionalProperties/patternProperties rather than fixed properties.
DICT = {
    "pyproject": [
        r"^project\.(scripts|gui-scripts|entry-points|optional-dependencies|urls)$",
        r"^dependency-groups$",
        r"^tool$",
        r"^tool\.uv\.sources$",
        r"^tool\.pdm\.dev-dependencies$",
        r"^tool\.poetry\.(dependencies|group|plugins|scripts)$",
        r"per-file-ignores$",
    ],
    "Cargo": [
        r"(^|\.)(dependencies|dev-dependencies|build-dependencies)$",
        r"^features$",
        r"^target$",
        r"^profile$",
        r"^package\.metadata$",
        r"^workspace\.(dependencies|metadata)$",
        r"^lints$",
        r"^patch$",
    ],
    "workflows": [
        r"^jobs$",
        r"(^|\.)env$",
        r"\.with$",
        r"\.outputs$",
        r"inputs$",
        r"\.matrix$",
        r"\.services$",
        r"\.secrets$",
    ],
}


def paths_dict_aware(x, rules, pre=""):
    out = set()
    if isinstance(x, dict):
        is_dict = any(re.search(r, pre) for r in rules) if pre else False
        for k, v in x.items():
            p = f"{pre}.*" if is_dict else (f"{pre}.{k}" if pre else str(k))
            out.add(p)
            out |= paths_dict_aware(v, rules, p)
    elif isinstance(x, list):
        for v in x:
            if isinstance(v, (dict, list)):
                out |= paths_dict_aware(v, rules, pre + "[]")
    return out


for nm, pred, key in [
    ("pyproject.toml", lambda r: r == "pyproject.toml", "pyproject"),
    ("Cargo.toml", lambda r: r == "cargo.toml", "Cargo"),
    ("workflows", lambda r: r.startswith(".github/workflows/"), "workflows"),
]:
    docs = A.load(pred)
    A.paths = lambda x, pre="": paths_dict_aware(x, DICT[key], pre)
    P = A.spectrum(nm + "  [dictionary-aware]", docs)
    A.clusters(nm, P)

# FCA attribute budget: how much of it is high-cardinality value holes (versions/pins)?
for f in ["pre-commit", "pyproject", "Cargo", "workflows"]:
    g = json.load(open(A.exp / f"au_{f}.json"))["groups"][0]
    nd = [
        len(h["distinct_values"]) + (1 if h["present_in"] < h["total"] else 0)
        for h in g["holes"]
    ]
    hi = [n for n in nd if n >= 8]
    ver = sum(
        1
        for h in g["holes"]
        if re.search(r"(^|\.)(rev|version|uses|requires-python)$|\.rev$", h["address"])
    )
    print(
        f"\n{f}: holes={len(nd)} attributes~{sum(nd)}  holes with >=8 values={len(hi)} carrying {sum(hi)} attrs ({sum(hi) / max(1, sum(nd)):.0%})  version-ish holes={ver}"
    )
    top = sorted(g["holes"], key=lambda h: -len(h["distinct_values"]))[:4]
    for h in top:
        print(f"   {len(h['distinct_values']):3d} values  {h['address'][:70]}")
