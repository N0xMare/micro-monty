import asyncio, json, re

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-14"}) for s in services))
err_re = re.compile(r'^\S+\s+ERROR\s')
counts = {s: sum(1 for line in text.splitlines() if err_re.match(line)) for s, text in zip(services, logs)}
print(counts)
print(await submit_answer({"counts": counts}))
