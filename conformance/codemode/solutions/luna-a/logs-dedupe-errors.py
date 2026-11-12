import asyncio
import re
from collections import Counter

text = await fetch_logs({"service": "notifications", "date": "2026-03-13"})
counts = Counter()
for line in text.splitlines():
    if "ERROR" not in line:
        continue
    match = re.search(r"notifications\[\d+\]:\s*(.*)$", line)
    if match:
        normalized = re.sub(r"\d+", "N", match.group(1))
        counts[normalized] += 1
messages = [{"message": msg, "count": n} for msg, n in sorted(counts.items(), key=lambda item: (-item[1], item[0]))]
await submit_answer({"messages": messages})
