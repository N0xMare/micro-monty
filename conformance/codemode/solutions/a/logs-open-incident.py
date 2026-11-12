import asyncio, json, re

DATE = "2026-03-15"
services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": DATE}) for s in services))
err_re = re.compile(r'^\S+\s+ERROR\s')
counts = {s: sum(1 for line in text.splitlines() if err_re.match(line)) for s, text in zip(services, logs)}
print("ERROR counts:", counts)
affected = sorted(s for s, n in counts.items() if n > 12)
if affected:
    result = json.loads(await create_incident({
        "title": "Elevated errors on 2026-03-15",
        "severity": "sev2",
        "services": affected,
        "summary": "ERROR lines on 2026-03-15 exceeded 12 for: "
                   + ", ".join(f"{s} ({counts[s]})" for s in affected),
    }))
    print("Opened incident:", result)
else:
    print("No service exceeded 12 ERROR lines; no incident opened.")
