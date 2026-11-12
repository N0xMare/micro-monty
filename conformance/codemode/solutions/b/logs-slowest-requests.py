import re

text = await fetch_logs({"service": "auth", "date": "2026-03-14"})
line_re = re.compile(r'^(\S+)\s+(\w+)\s+auth\[\d+\]: (.*)$')
kv_re = re.compile(r"(\w+)=(\S+)")
reqs = []
for line in text.splitlines():
    m = line_re.match(line)
    if not m:
        continue
    kv = dict(kv_re.findall(m.group(3)))
    if "latency_ms" in kv and "path" in kv and re.fullmatch(r"\d{3}", kv.get("status", "")):
        reqs.append({"ts": m.group(1), "path": kv["path"], "latency_ms": int(float(kv["latency_ms"].rstrip("ms")))})
reqs.sort(key=lambda r: (-r["latency_ms"], r["ts"]))
slowest = reqs[:5]
print(len(reqs), "completed requests;", slowest)
print(await submit_answer({"slowest": slowest}))
