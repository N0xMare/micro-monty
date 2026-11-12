import re
from collections import Counter

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
line_re = re.compile(r'^(\S+)\s+(\w+)\s+[\w.-]+\[\d+\]: (.*)$')
status_re = re.compile(r"\bstatus=(\d{3})\b")
total, bad = Counter(), Counter()
for line in text.splitlines():
    m = line_re.match(line)
    if not m:
        continue
    s = status_re.search(m.group(3))
    if not s:
        continue
    hour = m.group(1)[11:13]
    total[hour] += 1
    if s.group(1).startswith("5"):
        bad[hour] += 1
rates = {h: round(100 * bad[h] / total[h], 1) for h in sorted(total)}
print(rates)
print(await submit_answer({"rates": rates}))
