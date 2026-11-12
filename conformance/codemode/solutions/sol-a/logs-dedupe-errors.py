import re
from collections import Counter

log = await fetch_logs({"service": "notifications", "date": "2026-03-13"})
counts = Counter()
for line in log.splitlines():
    match = re.match(r'^\S+\s+ERROR\s+notifications\[\d+\]: (.*)$', line)
    if match:
        counts[re.sub(r'\d+', 'N', match[1])] += 1
messages = [{"message": message, "count": count} for message, count in sorted(counts.items(), key=lambda item: (-item[1], item[0]))]
await submit_answer({"messages": messages})
