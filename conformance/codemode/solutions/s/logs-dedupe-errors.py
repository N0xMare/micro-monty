import re
from collections import Counter

text = await fetch_logs({"service": "notifications", "date": "2026-03-13"})
line_re = re.compile(r'^(\S+)\s+(DEBUG|INFO|WARN|ERROR)\s+notifications\[\d+\]: (.*)$')
counts = Counter()
for line in text.splitlines():
    m = line_re.match(line.rstrip("\r"))
    if not m or m.group(2) != "ERROR":
        continue
    counts[re.sub(r'\d+', 'N', m.group(3))] += 1
messages = [{"message": msg, "count": n}
            for msg, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))]
for row in messages:
    print(row)
print(await submit_answer({"messages": messages}))
