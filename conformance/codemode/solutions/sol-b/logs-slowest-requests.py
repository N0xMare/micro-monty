import re

log = await fetch_logs({"service": "auth", "date": "2026-03-14"})
requests = []
for line in log.splitlines():
    if re.search(r"\b(?:request completed|completed request)\b", line):
        fields = dict(re.findall(r"\b(\w+)=(\S+)", line))
        requests.append({"ts": line.split()[0], "path": fields["path"], "latency_ms": int(fields["latency_ms"])})
requests.sort(key=lambda item: (-item["latency_ms"], item["ts"]))
await submit_answer({"slowest": requests[:5]})
