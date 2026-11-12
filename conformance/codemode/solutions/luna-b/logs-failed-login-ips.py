import asyncio
import re
from collections import Counter

dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "auth", "date": date}) for date in dates))
failures = Counter()
for text in logs:
    for line in text.splitlines():
        if "login failed" not in line.lower():
            continue
        match = re.search(r"\b(?:ip|client_ip|remote_addr)=([0-9A-Fa-f:.]+)", line)
        if not match:
            match = re.search(r"login failed from ([0-9A-Fa-f:.]+)", line, re.IGNORECASE)
        if match:
            failures[match.group(1)] += 1
ips = [{"ip": ip, "failures": count} for ip, count in sorted(failures.items(), key=lambda item: (-item[1], item[0])) if count >= 3]
await submit_answer({"ips": ips})
