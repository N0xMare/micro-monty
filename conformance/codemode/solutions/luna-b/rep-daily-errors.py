import asyncio, json

services = json.loads(await list_services({}))
results = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
counts = {service: sum(1 for line in log.splitlines() if " ERROR " in line) for service, log in zip(services, results)}
total = sum(counts.values())
ordered = sorted(counts.items(), key=lambda item: (-item[1], item[0]))
lines = ["Daily error report for 2026-03-15", ""]
lines.extend(f"- {service}: {count} errors ({count / total * 100 if total else 0:.1f}%)" for service, count in ordered)
lines.extend(["", f"Total: {total}"])
await post_message({"channel": "#ops", "text": "\n".join(lines)})
