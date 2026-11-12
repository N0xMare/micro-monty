import asyncio
import json
import re

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
affected = []
for service, text in zip(services, logs):
    if sum(bool(re.search(r"\sERROR\s", line)) for line in text.splitlines()) > 12:
        affected.append(service)
if affected:
    await create_incident({"title": "Elevated errors on 2026-03-15", "severity": "sev2", "services": sorted(affected)})
