import asyncio
import math
import re
import statistics

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": day}) for day in days))
latencies = []
for log in logs:
    for line in log.splitlines():
        if "completed" not in line.lower(): continue
        m = re.search(r"\blatency_ms=(\d+(?:\.\d+)?)\b", line)
        if m: latencies.append(float(m.group(1)))
values = sorted(latencies)
p95 = values[math.ceil(0.95 * len(values)) - 1]
await submit_answer({"mean": round(statistics.mean(values), 1), "median": statistics.median(values), "p95": p95})
