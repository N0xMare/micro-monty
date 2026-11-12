import asyncio
import re

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
level_re = re.compile(r'^\S+\s+(DEBUG|INFO|WARN|ERROR)\s')
counts = {}
for svc, text in zip(services, logs):
    counts[svc] = sum(1 for line in text.splitlines()
                      if (m := level_re.match(line)) and m.group(1) == "ERROR")
print("ERROR counts:", counts)
affected = sorted(s for s, n in counts.items() if n > 12)
if affected:
    result = await create_incident({
        "title": "Elevated errors on 2026-03-15",
        "severity": "sev2",
        "services": affected,
        "summary": "ERROR lines on 2026-03-15: "
                   + ", ".join(f"{s}={counts[s]}" for s in affected),
    })
    print("Opened incident:", result)
else:
    print("No service exceeded 12 ERROR lines; no incident opened.")
