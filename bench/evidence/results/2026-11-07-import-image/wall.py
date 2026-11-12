import json, subprocess, os, sys
mods = ["-", "json", "re", "collections", "asyncio", "json,re,collections", "json,re,collections,asyncio,datetime"]
res = {}
for rnd in range(3):
    for cfg in ["base", "img"]:
        for cache in ["on", "off"]:
            env = dict(os.environ)
            if cache == "off": env["IMP_CACHE"] = "0"
            for m in mods:
                out = subprocess.run(["taskset", "-c", "2", f"target/import-cost-{cfg}/release/imp", "time", m, "51"], env=env, capture_output=True, text=True, check=True).stdout
                d = json.loads(out)
                k = (cfg, cache, m)
                r = res.setdefault(k, {"median": [], "first": []})
                r["median"].append(d["median_ns"]); r["first"].append(d["first_ns"])
out = {}
for (cfg, cache, m), r in res.items():
    out[f"{cfg}|{cache}|{m}"] = {"median_min": min(r["median"]), "first_min": min(r["first"]), "medians": r["median"], "firsts": r["first"]}
json.dump(out, open(sys.argv[1], "w"), indent=1)
for cache in ["on", "off"]:
    print(f"cache {cache}: | import | today µs | graft µs | today first µs | graft first µs |")
    for m in mods:
        b = out[f"base|{cache}|{m}"]; g = out[f"img|{cache}|{m}"]
        b0 = out[f"base|{cache}|-"]["median_min"]; g0 = out[f"img|{cache}|-"]["median_min"]
        sub = (lambda x, z: x - z) if m != "-" else (lambda x, z: x)
        print(f"| {m} | {sub(b['median_min'], b0)/1e3:.0f} | {sub(g['median_min'], g0)/1e3:.0f} | {b['first_min']/1e3:.0f} | {g['first_min']/1e3:.0f} |")
