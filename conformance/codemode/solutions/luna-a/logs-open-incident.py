import asyncio
import json

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
affected = sorted(service for service, text in zip(services, texts) if sum("ERROR" in line.split()[:2] for line in text.splitlines()) > 12)
if affected:
    await create_incident({"title": "Elevated errors on 2026-03-15", "severity": "sev2", "services": affected})
