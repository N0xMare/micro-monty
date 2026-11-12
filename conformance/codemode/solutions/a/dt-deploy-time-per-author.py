import json
from collections import defaultdict

deps = json.loads(await list_deployments({}))
secs = defaultdict(int)
for d in deps:
    secs[d["author"]] += d["duration_s"]

def fmt(s):
    h, rem = divmod(int(s), 3600)
    m, sec = divmod(rem, 60)
    return f"{h}:{m:02d}:{sec:02d}"

totals = {a: fmt(s) for a, s in sorted(secs.items())}
print(totals)
print(await submit_answer({"totals": totals}))
