import re

text = await fetch_logs({"service": "auth", "date": "2026-03-14"})
line_re = re.compile(r'^(\S+)\s+(DEBUG|INFO|WARN|ERROR)\s+\S+?\[\d+\]: (.*)$')
lat_re = re.compile(r'\blatency_ms=(\d+)')
path_re = re.compile(r'\bpath=(\S+)')
rows = []
for line in text.splitlines():
    m = line_re.match(line.rstrip("\r"))
    if not m:
        continue
    ts, msg = m.group(1), m.group(3)
    if not re.search(r'\bcompleted\b', msg, re.I):
        continue
    lat = lat_re.search(msg)
    path = path_re.search(msg)
    if not lat:
        continue
    rows.append({"ts": ts, "path": path.group(1).strip('"\'') if path else None,
                 "latency_ms": int(lat.group(1))})
rows.sort(key=lambda r: (-r["latency_ms"], r["ts"]))
slowest = rows[:5]
print(len(rows), "completed requests")
for r in slowest:
    print(r)
print(await submit_answer({"slowest": slowest}))
