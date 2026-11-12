import asyncio
import json

services = json.loads(await list_services({}))
days = ["2026-03-13", "2026-03-14", "2026-03-15"]
results = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service in services for day in days))
errors = {service: 0 for service in services}
for service, text in zip((s for s in services for _ in days), results):
    errors[service] += sum(len(line.split()) > 1 and line.split()[1] == "ERROR" for line in text.splitlines())
await submit_answer({"errors": errors})
