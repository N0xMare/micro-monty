import asyncio

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-14"}) for service in services))
counts = {}
for service, text in zip(services, logs):
    counts[service] = sum(len(line.split()) > 1 and line.split()[1] == "ERROR" for line in text.splitlines())
await submit_answer({"counts": counts})
