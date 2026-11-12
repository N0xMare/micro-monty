import asyncio
import re
from collections import Counter

logs = await asyncio.gather(*(fetch_logs({"service": "auth", "date": day}) for day in ("2026-03-13", "2026-03-14", "2026-03-15")))
counts = Counter()
for log in logs:
    for line in log.splitlines():
        if "login failed" in line:
            match = re.search(r'\bip=(\S+)', line)
            if match:
                counts[match[1]] += 1
await submit_answer({"ips": [{"ip": ip, "failures": count} for ip, count in sorted(counts.items(), key=lambda x: (-x[1], x[0])) if count >= 3]})
