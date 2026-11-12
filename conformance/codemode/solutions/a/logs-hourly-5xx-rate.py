import re
from collections import Counter

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
line_re = re.compile(r'^\d{4}-\d{2}-\d{2}T(\d{2}):\S*\s+\S+\s+gateway\[\d+\]: (.*)$')
status_re = re.compile(r'\bstatus=(\d{3})\b')
total, bad = Counter(), Counter()
samples = []
for line in text.splitlines():
    m = line_re.match(line.rstrip("\r"))
    if not m:
        continue
    hour, msg = m.groups()
    if "complet" not in msg.lower():
        continue
    s = status_re.search(msg)
    if not s:
        continue
    if len(samples) < 3:
        samples.append(line)
    total[hour] += 1
    if s.group(1).startswith("5"):
        bad[hour] += 1
print("samples:", *samples, sep="\n")
rates = {h: round(100 * bad[h] / total[h], 1) for h in sorted(total)}
print({h: (bad[h], total[h], rates[h]) for h in rates})
print(await submit_answer({"rates": rates}))
