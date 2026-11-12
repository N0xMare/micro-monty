import json
import asyncio

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
pairs = [(s, d) for s in services for d in dates]
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))
errors = {s: 0 for s in services}
for (s, d), text in zip(pairs, texts):
    for line in text.splitlines():
        parts = line.split(None, 2)
        if len(parts) >= 2 and parts[1] == "ERROR":
            errors[s] += 1
print(errors)
await submit_answer({"errors": errors})
