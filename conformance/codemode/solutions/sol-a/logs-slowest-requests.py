import re

log = await fetch_logs({"service": "auth", "date": "2026-03-14"})
requests = []
for line in log.splitlines():
    if "request completed" in line:
        fields = dict(re.findall(r'\b(\w+)=(\S+)', line))
        if "path" in fields and "latency_ms" in fields:
            requests.append({"ts": line.split()[0], "path": fields["path"], "latency_ms": int(fields["latency_ms"])})
requests.sort(key=lambda row: (-row["latency_ms"], row["ts"]))
await submit_answer({"slowest": requests[:5]})
