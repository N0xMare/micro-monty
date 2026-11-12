import asyncio
import re
from statistics import mean, median

logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": date}) for date in ("2026-03-13", "2026-03-14", "2026-03-15")))
values = []
for log in logs:
    for line in log.splitlines():
        if re.search(r"\brequest(?:[ _-]+)completed\b|\bcompleted(?:[ _-]+)request\b", line, re.I):
            match = re.search(r"\blatency_ms=(\d+(?:\.\d+)?)", line)
            if match:
                values.append(float(match[1]))
values.sort()
await submit_answer({"mean": round(mean(values), 1), "median": median(values), "p95": values[(95 * len(values) + 99) // 100 - 1]})
