import re
from collections import Counter

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
line_re = re.compile(r'^(\S+)\s+(DEBUG|INFO|WARN|ERROR)\s+\S+?\[\d+\]: (.*)$')
status_re = re.compile(r'\bstatus=(\d{3})\b')
total = Counter()
bad = Counter()
samples = []
for line in text.splitlines():
    m = line_re.match(line.rstrip("\r"))
    if not m:
        continue
    ts, msg = m.group(1), m.group(3)
    if not re.search(r'\bcompleted\b', msg, re.I):
        continue
    s = status_re.search(msg)
    if not s or not ts.startswith("2026-03-14T"):
        continue
    if len(samples) < 3:
        samples.append(line)
    hour = ts[11:13]
    total[hour] += 1
    if s.group(1).startswith("5"):
        bad[hour] += 1
print(*samples, sep="\n")
rates = {h: round(100 * bad[h] / total[h], 1) for h in sorted(total)}
print(rates)
print(await submit_answer({"rates": rates}))
