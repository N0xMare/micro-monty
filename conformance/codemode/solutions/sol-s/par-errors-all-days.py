import asyncio
import re

services = await list_services({})
keys = [(service, day) for service in services for day in ("2026-03-13", "2026-03-14", "2026-03-15")]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service, day in keys))
errors = dict.fromkeys(services, 0)
for (service, day), log in zip(keys, logs):
    errors[service] += sum(bool(re.match(r'^\S+\s+ERROR\s+', line)) for line in log.splitlines())
await submit_answer({"errors": errors})
