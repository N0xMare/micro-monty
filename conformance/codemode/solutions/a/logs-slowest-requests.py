import re
from datetime import datetime

text = await fetch_logs({"service": "auth", "date": "2026-03-14"})
line_re = re.compile(r'^(\S+)\s+\S+\s+auth\[\d+\]: (.*)$')
lat_re = re.compile(r'\blatency_ms=(\d+)\b')
path_re = re.compile(r'\bpath=(\S+)')
reqs = []
for line in text.splitlines():
    m = line_re.match(line.rstrip("\r"))
    if not m:
        continue
    ts, msg = m.groups()
    if "complet" not in msg.lower():
        continue
    lat, path = lat_re.search(msg), path_re.search(msg)
    if not lat or not path:
        print("skipped (missing fields):", line)
        continue
    t = datetime.fromisoformat(ts.replace("Z", "+00:00"))
    reqs.append((-int(lat.group(1)), t, ts, path.group(1).strip('"')))
reqs.sort()
slowest = [{"ts": ts, "path": p, "latency_ms": -neg} for neg, _, ts, p in reqs[:5]]
print(len(reqs), "completed requests")
for r in slowest:
    print(r)
print(await submit_answer({"slowest": slowest}))
