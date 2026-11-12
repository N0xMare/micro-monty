import asyncio
import re
from collections import Counter

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "auth", "date": d}) for d in days))
ip_kv = re.compile(r'\b(?:ip|src|client|remote_addr|addr)=["\']?([0-9a-fA-F:.]+)')
ipv4 = re.compile(r'\b(\d{1,3}(?:\.\d{1,3}){3})\b')
counts = Counter()
missing = 0
for text in logs:
    for line in text.splitlines():
        if "login failed" not in line.lower():
            continue
        m = ip_kv.search(line) or ipv4.search(line)
        if not m:
            missing += 1
            continue
        counts[m.group(1)] += 1
ips = [{"ip": ip, "failures": n}
       for ip, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])) if n >= 3]
print(ips, "lines without ip:", missing)
print(await submit_answer({"ips": ips}))
