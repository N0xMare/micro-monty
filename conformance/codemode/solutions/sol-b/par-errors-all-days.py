import asyncio
import json
import re

services = json.loads(await list_services({}))
jobs = [(service, date) for service in services for date in ("2026-03-13", "2026-03-14", "2026-03-15")]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": date}) for service, date in jobs))
errors = dict.fromkeys(services, 0)
for (service, date), log in zip(jobs, logs):
    errors[service] += sum(bool(re.match(r"\S+\s+ERROR\s", line)) for line in log.splitlines())
await submit_answer({"errors": errors})
