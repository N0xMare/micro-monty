import asyncio, re
from collections import Counter

dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "auth", "date": d}) for d in dates))
ip_re = re.compile(r'\b(?:ip|client_ip|src|remote_addr|addr)=(\d{1,3}(?:\.\d{1,3}){3})\b')
any_ip_re = re.compile(r'\b(\d{1,3}(?:\.\d{1,3}){3})\b')
counts = Counter()
no_ip = []
for text in logs:
    for line in text.splitlines():
        if "login failed" not in line.lower():
            continue
        m = ip_re.search(line) or any_ip_re.search(line)
        if m:
            counts[m.group(1)] += 1
        else:
            no_ip.append(line)
if no_ip:
    print("login failed lines without IP:", len(no_ip), no_ip[:3])
ips = [{"ip": ip, "failures": n}
       for ip, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])) if n >= 3]
print(ips)
print(await submit_answer({"ips": ips}))
