import asyncio, json

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
counts = {}
for s, text in zip(services, texts):
    counts[s] = sum(1 for line in text.splitlines() if len(line.split()) > 1 and line.split()[1] == "ERROR")
print("ERROR lines on 2026-03-15:", counts)
affected = sorted(s for s, n in counts.items() if n > 12)
if affected:
    result = json.loads(await create_incident({
        "title": "Elevated errors on 2026-03-15",
        "severity": "sev2",
        "services": affected,
        "summary": "ERROR lines on 2026-03-15 above threshold 12: "
                   + ", ".join(f"{s}={counts[s]}" for s in affected),
    }))
    print("incident opened:", result)
else:
    print("No service exceeded 12 ERROR lines; no incident opened.")
# Note: no submit_answer tool is in this catalog, so the result is reported via print.
