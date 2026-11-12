import asyncio, re
from collections import Counter

dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
texts = await asyncio.gather(*(fetch_logs({"service": "auth", "date": d}) for d in dates))
ip_kv = re.compile(r"\b(?:ip|src|client|remote_addr|addr)=(\d{1,3}(?:\.\d{1,3}){3})")
ip_any = re.compile(r"\b(\d{1,3}(?:\.\d{1,3}){3})\b")
fails = Counter()
no_ip = 0
for text in texts:
    for line in text.splitlines():
        if "login failed" not in line:
            continue
        m = ip_kv.search(line) or ip_any.search(line)
        if m:
            fails[m.group(1)] += 1
        else:
            no_ip += 1
ips = [{"ip": ip, "failures": n}
       for ip, n in sorted(fails.items(), key=lambda kv: (-kv[1], kv[0])) if n >= 3]
print(ips, "lines without ip:", no_ip)
print(await submit_answer({"ips": ips}))
