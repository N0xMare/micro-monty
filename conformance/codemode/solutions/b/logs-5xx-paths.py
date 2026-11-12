import asyncio, re
from collections import Counter

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": d}) for d in days))

kv_re = re.compile(r'(\w+)=("[^"]*"|\S+)')
req_re = re.compile(r'\b(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\s+(/\S*)\s+(?:\S+\s+)?(\d{3})\b')

paths = Counter()
for text in logs:
    for line in text.splitlines():
        kv = {k.lower(): v.strip('"') for k, v in kv_re.findall(line)}
        status = kv.get("status") or kv.get("status_code") or kv.get("code")
        path = kv.get("path") or kv.get("route") or kv.get("uri") or kv.get("url")
        if status is None or path is None:
            m = req_re.search(line)
            if m:
                path = path or m.group(1)
                status = status or m.group(2)
        if status is None or path is None or not status.isdigit():
            continue
        if int(status) >= 500:
            paths[path.split("?", 1)[0]] += 1

print(sum(len(t.splitlines()) for t in logs), "lines")
print(logs[0].splitlines()[:3])
result = dict(paths.most_common())
print(result)
print(await submit_answer({"paths": result}))
