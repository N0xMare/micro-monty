import asyncio, json

# Dumping every full log here would flood the output; count all lines and show a short preview of each log.
services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
pairs = [(s, d) for s in services for d in dates]
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in pairs))
total = 0
for (s, d), text in zip(pairs, logs):
    lines = [l for l in text.splitlines() if l.strip()]
    total += len(lines)
    print(f"=== {s} {d}: {len(lines)} lines")
    for l in lines[:5]:
        print(l)
    if len(lines) > 5:
        print(f"... ({len(lines) - 5} more)")
print("TOTAL lines:", total)
print(await submit_answer({"lines": total}))
