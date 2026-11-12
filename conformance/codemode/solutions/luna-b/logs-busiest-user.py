import asyncio
import json
import re

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-14"}) for service in services))
counts = {}
for text in logs:
    for line in text.splitlines():
        if "request" not in line.lower() or "completed" not in line.lower():
            continue
        match = re.search(r"\buser=([^\s]+)", line)
        if match:
            user = match.group(1).strip('"\' ,')
            counts[user] = counts.get(user, 0) + 1
if counts:
    user, requests = min(counts.items(), key=lambda item: (-item[1], item[0]))
    await submit_answer({"user": user, "requests": requests})
else:
    await submit_answer({"user": "", "requests": 0})
