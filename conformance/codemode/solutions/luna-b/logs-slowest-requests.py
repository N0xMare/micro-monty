import re

text = await fetch_logs({"service": "auth", "date": "2026-03-14"})
requests = []
for line in text.splitlines():
    if "request" not in line.lower() or "completed" not in line.lower():
        continue
    ts = re.match(r"^(\S+)", line)
    path = re.search(r"\bpath=([^\s]+)", line)
    latency = re.search(r"\blatency_ms=(\d+)", line)
    if ts and path and latency:
        requests.append({"ts": ts.group(1), "path": path.group(1), "latency_ms": int(latency.group(1))})
slowest = sorted(requests, key=lambda item: (-item["latency_ms"], item["ts"]))[:5]
await submit_answer({"slowest": slowest})
