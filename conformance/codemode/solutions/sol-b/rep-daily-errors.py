import asyncio, json

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
counts = {s: sum(len(parts := line.split()) > 1 and parts[1] == "ERROR" for line in log.splitlines()) for s, log in zip(services, logs)}
total = sum(counts.values())
lines = ["Daily error report for 2026-03-15", ""]
for service, n in sorted(counts.items(), key=lambda p: (-p[1], p[0])):
    share = n / total * 100 if total else 0
    lines.append(f"- {service}: {n} errors ({share:.1f}%)")
lines.extend(["", f"Total: {total}"])
await post_message({"channel": "#ops", "text": "\n".join(lines)})
