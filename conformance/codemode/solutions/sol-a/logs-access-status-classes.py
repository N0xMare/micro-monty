import re

log = await get_access_log({"date": "2026-03-14"})
counts = dict.fromkeys(["2xx", "3xx", "4xx", "5xx"], 0)
for line in log.splitlines():
    match = re.search(r'"\s+(\d{3})\s+', line)
    if match:
        key = match[1][0] + "xx"
        if key in counts:
            counts[key] += 1
await submit_answer({"classes": counts})
