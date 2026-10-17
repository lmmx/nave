"""Cross-tabulate the k=6 dictionary-aware workflow clusters (as in analyse3.py) against filenames."""
import collections, contextlib, io, sys
from pathlib import Path
import numpy as np
from scipy.cluster.hierarchy import linkage, fcluster
from scipy.spatial.distance import squareform
sys.path.insert(0, str(Path(__file__).parent))
with contextlib.redirect_stdout(io.StringIO()):
    import analyse as A, analyse2 as B

docs = A.load(lambda r: r.startswith(".github/workflows/"))
P = {d: B.paths_dict_aware(v, B.DICT["workflows"]) for d, v in docs.items()}
keys = list(P); n = len(keys)
D = np.array([[1 - len(P[a] & P[b]) / max(1, len(P[a] | P[b])) for b in keys] for a in keys])
lab = fcluster(linkage(squareform(D, checks=False), "average"), 6, "maxclust")
order = sorted(set(lab), key=lambda c: -sum(lab == c))
cid = {c: f"C{i+1}" for i, c in enumerate(order)}
fn = [Path(f).name for _, f in keys]
tab = collections.defaultdict(collections.Counter)
for f, l in zip(fn, lab): tab[f][cid[l]] += 1
cols = [cid[c] for c in order]
print("filename".ljust(22), " ".join(c.rjust(4) for c in cols), "  n")
for f in sorted(tab, key=lambda f: -sum(tab[f].values())):
    print(f.ljust(22), " ".join(str(tab[f][c] or ".").rjust(4) for c in cols), f"{sum(tab[f].values()):3d}")
print("cluster sizes".ljust(22), " ".join(str(sum(lab == c)).rjust(4) for c in order))
acc_f2c = sum(max(c.values()) for c in tab.values()) / n
by_c = collections.defaultdict(collections.Counter)
for f, l in zip(fn, lab): by_c[cid[l]][f] += 1
acc_c2f = sum(max(c.values()) for c in by_c.values()) / n
print(f"\nfilename -> majority cluster accuracy: {acc_f2c:.2f} (majority-class baseline {max(collections.Counter(lab).values())/n:.2f})")
print(f"cluster -> majority filename accuracy: {acc_c2f:.2f}  distinct filenames={len(tab)}")
print("\nmembers by cluster (repo/file):")
for c in order:
    print(f"  {cid[c]}: " + ", ".join(f"{r}/{Path(f).name}" for (r, f), l in zip(keys, lab) if l == c))
