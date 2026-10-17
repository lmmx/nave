import collections, re, sys
from pathlib import Path
import numpy as np
from scipy.cluster.hierarchy import linkage, fcluster
from scipy.spatial.distance import squareform
sys.path.insert(0, str(Path(__file__).parent))
import analyse as A, analyse2 as B

def show(nm, pred, key, k, describe):
    docs = A.load(pred)
    P = {d: B.paths_dict_aware(v, B.DICT[key]) for d, v in docs.items()}
    keys = list(P); n = len(keys)
    D = np.array([[1 - len(P[a] & P[b]) / max(1, len(P[a] | P[b])) for b in keys] for a in keys])
    lab = fcluster(linkage(squareform(D, checks=False), "average"), k, "maxclust")
    c_all = collections.Counter(p for s in P.values() for p in s)
    print(f"\n### {nm} k={k}")
    for c in sorted(set(lab), key=lambda c: -sum(lab == c)):
        m = [keys[i] for i in range(n) if lab[i] == c]
        if len(m) < 3: continue
        cc = collections.Counter(p for d in m for p in P[d])
        # paths most over-represented in this cluster relative to fleet
        lift = sorted(((cc[p]/len(m)) - (c_all[p]-cc[p])/max(1, n-len(m)), p) for p in cc)[::-1][:4]
        print(f"  [{len(m)}] {describe(m, docs)}")
        print("       distinguishing:", ", ".join(f"{p} (+{d:.0%})" for d, p in lift))

show("workflows", lambda r: r.startswith(".github/workflows/"), "workflows", 6,
     lambda m, d: dict(collections.Counter(Path(f).name for _, f in m).most_common(4)))
show("Cargo.toml", lambda r: r == "cargo.toml", "Cargo", 2,
     lambda m, d: f"pyo3 in deps: {sum('pyo3' in str(d[x].get('dependencies', {})) for x in m)}/{len(m)}, workspace: {sum('workspace' in d[x] for x in m)}")
