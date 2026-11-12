import re

text = await fetch_logs({"service": "auth", "date": "2026-03-14"})
requests = []
for line in text.splitlines():
    if "request completed" not in line:
        continue
    ts_match = re.match(r"^(\S+)", line)
    path_match = re.search(r"\bpath=([^\s]+)", line)
    latency_match = re.search(r"\blatency_ms=(\d+)\b", line)
    if ts_match and path_match and latency_match:
        requests.append({"ts": ts_match.group(1), "path": path_match.group(1), "latency_ms": int(latency_match.group(1))})
slowest = sorted(requests, key=lambda row: (-row["latency_ms"], row["ts"]))[:5]
await submit_answer({"slowest": slowest})
