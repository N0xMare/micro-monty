import re
from collections import Counter

text = await fetch_logs({"service": "notifications", "date": "2026-03-13"})
line_re = re.compile(r'^(\S+)\s+(\w+)\s+notifications\[\d+\]: (.*)$')
counts = Counter()
for line in text.splitlines():
    m = line_re.match(line)
    if m and m.group(2) == "ERROR":
        counts[re.sub(r"\d+", "N", m.group(3))] += 1
messages = [{"message": msg, "count": n} for msg, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))]
print(messages)
print(await submit_answer({"messages": messages}))
