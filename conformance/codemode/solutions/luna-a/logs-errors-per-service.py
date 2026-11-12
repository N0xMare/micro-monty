import asyncio
import json

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-14"}) for service in services))
counts = {service: sum("ERROR" in line.split()[:2] for line in text.splitlines()) for service, text in zip(services, texts)}
await submit_answer({"counts": counts})
