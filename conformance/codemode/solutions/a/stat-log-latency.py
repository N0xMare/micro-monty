import asyncio
import math
import re
import statistics

dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": d}) for d in dates))

lat_re = re.compile(r"\blatency_ms=([0-9]+(?:\.[0-9]+)?)")
latencies = []
sample_lines = []
for text in logs:
    for line in text.splitlines():
        m = lat_re.search(line)
        if not m:
            continue
        if len(sample_lines) < 5:
            sample_lines.append(line)
        # "Completed" request: a completion message or status, not an error/timeout.
        low = line.lower()
        if "complet" in low or "status=ok" in low or "status=200" in low:
            latencies.append(float(m.group(1)))

for s in sample_lines:
    print(s)

latencies.sort()
n = len(latencies)
p95 = latencies[math.ceil(0.95 * n) - 1]
median = statistics.median(latencies)


def clean(x):
    return int(x) if float(x).is_integer() else x


answer = {"mean": round(statistics.mean(latencies), 1), "median": clean(median), "p95": clean(p95)}
print(n, answer)
print(await submit_answer(answer))
