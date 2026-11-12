import asyncio
import re
from collections import Counter

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": d}) for d in days))
lines = [l for log in logs for l in log.splitlines() if l.strip()]
print(len(lines))
print("\n".join(lines[:8]))

kv_re = re.compile(r'(\w+)=("[^"]*"|\S+)')
req_re = re.compile(r'\b(?:GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)\s+(/\S*)\s+(?:HTTP/\S+\s+)?(\d{3})\b')
paths = Counter()
for l in lines:
    kv = {k: v.strip('"') for k, v in kv_re.findall(l)}
    path = kv.get("path") or kv.get("route") or kv.get("uri") or kv.get("url")
    status = kv.get("status") or kv.get("status_code") or kv.get("code")
    if path is None or status is None:
        m = req_re.search(l)
        if m:
            path = path or m.group(1)
            status = status or m.group(2)
    if path is None or status is None or not str(status).isdigit():
        continue
    if int(status) >= 500:
        paths[path.split("?", 1)[0]] += 1
result = dict(paths.most_common())
print(result)
print(await submit_answer({"paths": result}))
