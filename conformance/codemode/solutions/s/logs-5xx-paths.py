import asyncio, re, shlex
from collections import Counter

days = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": "search", "date": d}) for d in days))
print(logs[0].splitlines()[:5])

kv_re = re.compile(r'(\w+)=("(?:[^"\\]|\\.)*"|\S+)')
req_re = re.compile(r"\b(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\s+(/\S*)")
paths = Counter()
for text in logs:
    for line in text.splitlines():
        if not line.strip():
            continue
        kv = {k.lower(): v.strip('"') for k, v in kv_re.findall(line)}
        status = kv.get("status") or kv.get("status_code") or kv.get("code") or kv.get("http_status")
        if status is None or not status.isdigit() or int(status) < 500:
            continue
        path = kv.get("path") or kv.get("route") or kv.get("uri") or kv.get("url")
        if path is None:
            m = req_re.search(line)
            path = m.group(1) if m else None
        if path is None:
            print("no path:", line)
            continue
        paths[path.split("?", 1)[0]] += 1
result = dict(paths.most_common())
print(result)
await submit_answer({"paths": result})
