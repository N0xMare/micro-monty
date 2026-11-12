import asyncio
import math
import re
import statistics

logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": day}) for day in ("2026-03-13", "2026-03-14", "2026-03-15")))
values = []
for log in logs:
    for line in log.splitlines():
        if "completed" not in line.lower():
            continue
        match = re.search(r"\blatency_ms=(\d+(?:\.\d+)?)", line)
        if match:
            values.append(float(match.group(1)))
values.sort()
p95 = values[math.ceil(0.95 * len(values)) - 1]
await submit_answer({"mean": round(statistics.mean(values), 1), "median": statistics.median(values), "p95": p95})
