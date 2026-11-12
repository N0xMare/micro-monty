import asyncio
import json

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
counts = {s: sum(len(parts := line.split()) > 1 and parts[1] == "ERROR" for line in log.splitlines()) for s, log in zip(services, logs)}
affected = sorted(s for s, count in counts.items() if count > 12)
if affected:
    print(await create_incident({"title": "Elevated errors on 2026-03-15", "severity": "sev2", "services": affected}))
else:
    print("No service had more than 12 ERROR lines; no incident opened.")
