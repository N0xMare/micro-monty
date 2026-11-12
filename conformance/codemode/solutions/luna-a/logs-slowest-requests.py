import re

text = await fetch_logs({"service": "auth", "date": "2026-03-14"})
requests = []
for line in text.splitlines():
    if "request completed" not in line:
        continue
    ts = re.match(r"(\S+)", line)
    path = re.search(r"\bpath=(\S+)", line)
    latency = re.search(r"\blatency_ms=(\d+)\b", line)
    if ts and path and latency:
        requests.append({"ts": ts.group(1), "path": path.group(1), "latency_ms": int(latency.group(1))})
requests.sort(key=lambda row: (-row["latency_ms"], row["ts"]))
await submit_answer({"slowest": requests[:5]})
