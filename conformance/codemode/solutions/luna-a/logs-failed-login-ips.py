import asyncio
import re
from collections import Counter

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
texts = await asyncio.gather(*(fetch_logs({"service": "auth", "date": day}) for day in days))
counts = Counter()
for text in texts:
    for line in text.splitlines():
        if "login failed" not in line.lower():
            continue
        match = re.search(r"\bip=(\S+)", line)
        if match:
            counts[match.group(1)] += 1
ips = [{"ip": ip, "failures": n} for ip, n in sorted(counts.items(), key=lambda item: (-item[1], item[0])) if n >= 3]
await submit_answer({"ips": ips})
