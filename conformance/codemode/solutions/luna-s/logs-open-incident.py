import asyncio

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
affected = []
for service, text in zip(services, logs):
    error_count = sum(len(line.split()) > 1 and line.split()[1] == "ERROR" for line in text.splitlines())
    if error_count > 12:
        affected.append(service)
if affected:
    await create_incident({
        "title": "Elevated errors on 2026-03-15",
        "severity": "sev2",
        "services": sorted(affected),
    })
