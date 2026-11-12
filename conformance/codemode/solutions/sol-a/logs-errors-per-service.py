import asyncio
import json

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-14"}) for service in services))
counts = {service: sum(len(parts := line.split()) >= 2 and parts[1] == "ERROR" for line in log.splitlines()) for service, log in zip(services, logs)}
await submit_answer({"counts": counts})
