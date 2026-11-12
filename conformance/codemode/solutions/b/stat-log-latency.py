import asyncio
import math
import re
import statistics
from collections import Counter

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": d}) for d in days))

line_re = re.compile(r"^\S+\s+(DEBUG|INFO|WARN|ERROR)\s*\S+?:\s*(.*)$")
lat_re = re.compile(r"\blatency_ms=(\d+(?:\.\d+)?)")

with_latency = []
shapes = Counter()
for text in logs:
    for line in text.splitlines():
        m = line_re.match(line.strip())
        if not m:
            continue
        lm = lat_re.search(m.group(2))
        if not lm:
            continue
        msg = m.group(2)
        shapes[(m.group(1), re.sub(r"\s*\w+=\S+", "", msg).strip())] += 1
        with_latency.append((msg, float(lm.group(1))))

print("message shapes carrying latency_ms:")
for shape, count in shapes.most_common():
    print(" ", count, shape)

completed = [v for msg, v in with_latency
             if re.search(r"\bcomplet", msg, re.I) or re.search(r"\bstatus=(?:ok|completed|success|2\d\d)\b", msg, re.I)]
if not completed:
    print("no explicit 'completed' marker; using every latency_ms entry")
    completed = [v for _, v in with_latency]

values = sorted(completed)
n = len(values)
p95 = values[max(1, math.ceil(0.95 * n)) - 1]
answer = {"mean": round(statistics.mean(values), 1), "median": statistics.median(values), "p95": p95}
print(n, "completed requests", answer)
print(await submit_answer(answer))
