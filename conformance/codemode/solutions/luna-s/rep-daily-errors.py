import asyncio
services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
counts = {s: sum(1 for line in log.splitlines() if len(line.split()) > 1 and line.split()[1] == "ERROR") for s, log in zip(services, logs)}
total = sum(counts.values())
lines = ["Daily error report for 2026-03-15", ""]
for service, count in sorted(counts.items(), key=lambda x: (-x[1], x[0])):
    lines.append(f"- {service}: {count} errors ({(count / total * 100 if total else 0):.1f}%)")
lines.extend(["", f"Total: {total}"])
await post_message({"channel": "#ops", "text": "\n".join(lines)})
