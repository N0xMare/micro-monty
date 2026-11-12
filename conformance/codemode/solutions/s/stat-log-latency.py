import asyncio
import math
import re
import statistics

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*[fetch_logs({"service": "search", "date": d}) for d in days])

line_re = re.compile(r'^(\S+)\s+(DEBUG|INFO|WARN|ERROR)\s+(\S+?):\s*(.*)$')
lat_re = re.compile(r'\blatency_ms=(\d+(?:\.\d+)?)')

seen = set()
latencies = []
for d, text in zip(days, logs):
    for line in text.splitlines():
        line = line.rstrip()
        m = line_re.match(line)
        if not m or line in seen:
            continue
        seen.add(line)
        msg = m.group(4)
        if not re.search(r'\bcompleted\b', msg, re.I):
            continue
        lm = lat_re.search(msg)
        if lm:
            latencies.append(float(lm.group(1)))

n = len(latencies)
srt = sorted(latencies)
mean = statistics.mean(latencies)
median = statistics.median(latencies)
p95 = srt[math.ceil(0.95 * n) - 1]

def clean(x):
    return int(x) if float(x).is_integer() else x

print(n, mean, median, p95)
await submit_answer({"mean": round(mean, 1), "median": clean(median), "p95": clean(p95)})
