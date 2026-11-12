import asyncio
import re
import statistics
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": date}) for date in ["2026-03-13", "2026-03-14", "2026-03-15"]))
values = []
for log in logs:
    for line in log.splitlines():
        if re.search(r"\bcompleted\b", line, re.I):
            match = re.search(r"\blatency_ms=(\d+(?:\.\d+)?)", line)
            if match:
                values.append(float(match.group(1)))
values.sort()
await submit_answer({"mean": round(statistics.mean(values), 1), "median": statistics.median(values), "p95": values[(95 * len(values) + 99) // 100 - 1]})
