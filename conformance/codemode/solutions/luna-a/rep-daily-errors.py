import asyncio

day = "2026-03-15"
services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service in services))
counts = {service: sum(1 for line in log.splitlines() if " ERROR " in line) for service, log in zip(services, logs)}
total = sum(counts.values())
lines = [f"Daily error report for {day}", ""]
for service, count in sorted(counts.items(), key=lambda item: (-item[1], item[0])):
    share = count / total * 100 if total else 0.0
    lines.append(f"- {service}: {count} errors ({share:.1f}%)")
lines.extend(["", f"Total: {total}"])
await post_message({"channel": "#ops", "text": "\n".join(lines)})
