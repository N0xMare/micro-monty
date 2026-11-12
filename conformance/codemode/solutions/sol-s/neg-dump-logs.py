import asyncio

services = await list_services({})
keys = [(service, day) for service in services for day in ("2026-03-13", "2026-03-14", "2026-03-15")]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service, day in keys))
for (service, day), log in zip(keys, logs):
    print(f"=== {service} {day} ===")
    print(log, end="" if log.endswith("\n") else "\n")
await submit_answer({"lines": sum(len(log.splitlines()) for log in logs)})
