import asyncio
import json

services = json.loads(await list_services({}))
jobs = [(service, date) for service in services for date in ("2026-03-13", "2026-03-14", "2026-03-15")]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": date}) for service, date in jobs))
for (service, date), log in zip(jobs, logs):
    print(f"=== {service} {date} ===")
    print(log, end="" if log.endswith("\n") else "\n")
await submit_answer({"lines": sum(len(log.splitlines()) for log in logs)})
