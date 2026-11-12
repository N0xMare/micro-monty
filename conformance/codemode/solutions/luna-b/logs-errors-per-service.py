import asyncio
import json
import re

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
calls = [(service, date) for service in services for date in dates]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": date}) for service, date in calls))
errors = {service: 0 for service in services}
for (service, _), text in zip(calls, logs):
    errors[service] += sum(bool(re.search(r"\sERROR\s", line)) for line in text.splitlines())
await submit_answer({"errors": errors})
