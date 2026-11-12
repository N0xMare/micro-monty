import re
from collections import Counter

text = await fetch_logs({"service": "notifications", "date": "2026-03-13"})
counts = Counter()
for line in text.splitlines():
    if not re.search(r"\sERROR\s", line):
        continue
    match = re.search(r"notifications\[\d+\]:\s*(.*)$", line)
    if match:
        counts[re.sub(r"\d+", "N", match.group(1))] += 1
messages = [{"message": message, "count": count} for message, count in sorted(counts.items(), key=lambda item: (-item[1], item[0]))]
await submit_answer({"messages": messages})
