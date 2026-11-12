import re
import statistics
import math
import asyncio

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": day}) for day in days))
values = []
for log in logs:
    for line in log.splitlines():
        if re.search(r"\bcompleted\b", line, re.I):
            m = re.search(r"\blatency_ms=(\d+(?:\.\d+)?)\b", line)
            if m:
                values.append(float(m.group(1)))
values.sort()
p95 = values[max(0, math.ceil(.95 * len(values)) - 1)] if values else 0
await submit_answer({"mean": round(statistics.mean(values), 1) if values else 0.0, "median": statistics.median(values) if values else 0.0, "p95": p95})
