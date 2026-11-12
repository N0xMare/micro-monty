import asyncio
import re

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
affected = sorted(service for service, log in zip(services, logs) if sum(bool(re.match(r'^\S+\s+ERROR\s+', line)) for line in log.splitlines()) > 12)
if affected:
    incident = await create_incident({"title": "Elevated errors on 2026-03-15", "severity": "sev2", "services": affected})
    print(incident)
else:
    print("No services exceeded 12 ERROR lines; no incident opened.")
