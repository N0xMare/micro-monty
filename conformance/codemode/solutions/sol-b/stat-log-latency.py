import asyncio
import re
import math
import statistics
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": f"2026-03-{day}"}) for day in (13, 14, 15)))
values = []
for log in logs:
    for line in log.splitlines():
        if re.search(r"\bcompleted\b", line, re.I):
            match = re.search(r"\blatency_ms=(\d+(?:\.\d+)?)", line)
            if match:
                values.append(float(match.group(1)))
values.sort()
await submit_answer({"mean": round(statistics.mean(values), 1), "median": statistics.median(values), "p95": values[math.ceil(0.95 * len(values)) - 1]})
