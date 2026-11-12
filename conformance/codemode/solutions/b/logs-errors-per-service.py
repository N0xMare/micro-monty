import asyncio, json

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-14"}) for s in services))
counts = {}
for s, text in zip(services, texts):
    counts[s] = sum(1 for line in text.splitlines() if len(line.split()) > 1 and line.split()[1] == "ERROR")
print(counts)
print(await submit_answer({"counts": counts}))
