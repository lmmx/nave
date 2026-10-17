import json, glob, sys, tomllib, yaml, re, collections, datetime
from pathlib import Path
import numpy as np
from scipy.cluster.hierarchy import linkage, fcluster
from scipy.spatial.distance import squareform
from scipy.stats import spearmanr

exp = Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).parent.parent/"exp"; fleet = Path.home()/".cache/nave/fleet/lmmx"
meta = {r["full_name"].split("/")[1]: r for f in glob.glob(str(exp/"page*.json")) for r in json.load(open(f))}

def paths(x, pre=""):
    out = set()
    if isinstance(x, dict):
        for k, v in x.items():
            p = f"{pre}.{k}" if pre else str(k); out.add(p); out |= paths(v, p)
    elif isinstance(x, list):
        for v in x:
            if isinstance(v, (dict, list)): out |= paths(v, pre + "[]")
    return out

def load(kind):
    docs = {}
    for repo in sorted(fleet.iterdir()):
        co = repo/"checkout"
        if not co.exists(): continue
        for f in co.rglob("*"):
            rel = str(f.relative_to(co)).lower()
            if f.is_file() and kind(rel):
                try:
                    t = f.read_text()
                    d = tomllib.loads(t) if rel.endswith(".toml") else yaml.safe_load(t)
                    docs[(repo.name, str(f.relative_to(co)))] = d or {}
                except Exception: pass
    return docs

def spectrum(name, docs):
    P = {k: paths(v) for k, v in docs.items()}; n = len(P)
    c = collections.Counter(p for s in P.values() for p in s)
    fr = np.array([v/n for v in c.values()])
    print(f"\n== {name}: {n} files, {len(c)} distinct key paths")
    print(f"   universal(100%)={sum(fr==1)}  >=50%={sum(fr>=.5)}  10-50%={sum((fr>=.1)&(fr<.5))}  <10%={sum(fr<.1)}  singletons={sum(v==1 for v in c.values())}")
    return P

def clusters(name, P, label=None):
    keys = list(P); n = len(keys)
    D = np.zeros((n, n))
    for i in range(n):
        for j in range(i+1, n):
            a, b = P[keys[i]], P[keys[j]]
            D[i, j] = D[j, i] = 1 - len(a & b)/max(1, len(a | b))
    Z = linkage(squareform(D), "average")
    # silhouette for a few k
    def sil(lab):
        s = []
        for i in range(n):
            same = lab == lab[i]
            if same.sum() < 2: s.append(0); continue
            a = D[i, same].sum()/(same.sum()-1)
            b = min(D[i, lab == l].mean() for l in set(lab) if l != lab[i])
            s.append((b-a)/max(a, b))
        return np.mean(s)
    print(f"   mean pairwise Jaccard distance={D[np.triu_indices(n,1)].mean():.2f}")
    for k in (2, 3, 4, 6, 8):
        lab = fcluster(Z, k, "maxclust")
        sizes = sorted(collections.Counter(lab).values(), reverse=True)
        print(f"   k={k}: silhouette={sil(lab):.2f} sizes={sizes}")
    # time: Mantel-style correlation between config distance and creation-date distance
    t = np.array([datetime.datetime.fromisoformat(meta[r]["created_at"].replace("Z","")).timestamp()/86400/365.25 for r, _ in keys])
    T = np.abs(t[:, None] - t[None, :])
    iu = np.triu_indices(n, 1)
    rho, p = spearmanr(D[iu], T[iu])
    print(f"   config-distance vs creation-gap Spearman rho={rho:.2f} (pairs={len(iu[0])})")
    # permutation null for Mantel
    rng = np.random.default_rng(0); null = []
    for _ in range(200):
        perm = rng.permutation(n); null.append(spearmanr(D[iu], T[perm][:, perm][iu])[0])
    print(f"   permutation null: mean={np.mean(null):.3f} max={np.max(null):.3f}  -> p<{(1+sum(x>=rho for x in null))/201:.3f}")
    if label:
        labs = np.array([label(keys[i]) for i in range(n)])
        for k in (4, 6):
            lab = fcluster(Z, k, "maxclust")
            print(f"   k={k} clusters vs {label.__name__}:")
            for c_ in sorted(set(lab), key=lambda c_: -sum(lab == c_)):
                m = lab == c_
                yrs = sorted(int(2000+0) for _ in [0])
                print(f"     size {m.sum():3d}  years {int(t[m].min()+1970)}-{int(t[m].max()+1970)} (median {np.median(t[m])+1970:.1f})  {dict(collections.Counter(labs[m]).most_common(4))}")
    return keys, D, t

if __name__ == "__main__":
    docs = load(lambda r: r == "pyproject.toml")
    P = spectrum("pyproject.toml", docs)
    def backend(k):
        bs = docs[k].get("build-system", {}).get("build-backend", "none")
        return bs.split(".")[0].split(":")[0]
    clusters("pyproject", P, backend)
    print("\n   backend x creation year:")
    tab = collections.defaultdict(collections.Counter)
    for k in docs: tab[backend(k)][meta[k[0]]["created_at"][:4]] += 1
    for b, c in sorted(tab.items(), key=lambda x: -sum(x[1].values())): print(f"     {b:12s} {dict(sorted(c.items()))}")

    for nm, pred in [("pre-commit", lambda r: r.startswith(".pre-commit-config")),
                     ("Cargo.toml", lambda r: r == "cargo.toml"),
                     ("workflows", lambda r: r.startswith(".github/workflows/"))]:
        d = load(pred); P = spectrum(nm, d); clusters(nm, P)

    w = load(lambda r: r.startswith(".github/workflows/"))
    names = collections.Counter(Path(k[1]).name for k in w)
    print("\n   workflow filenames:", dict(names.most_common(10)))
    gen = {k: v for k, v in w.items() if Path(k[1]).name == "CI.yml"}
    print("   CI.yml files containing 'maturin generate-ci' header-ish (maturin-action present):",
          sum('maturin' in json.dumps(v) for v in gen.values()), "/", len(gen))
    spectrum("CI.yml only (maturin-generated lineage)", gen)
    tmpl = collections.Counter(meta[r].get("template_full_name") for r in meta if meta[r].get("template_full_name"))
    print("\n   repos created from a GitHub template:", dict(tmpl))
