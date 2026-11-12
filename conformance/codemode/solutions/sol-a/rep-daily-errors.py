import asyncio
import json

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": "2026-03-15"}) for service in services))
counts = {service: sum(len(parts := line.split()) >= 2 and parts[1] == "ERROR" for line in log.splitlines()) for service, log in zip(services, logs)}
total = sum(counts.values())
lines = ["Daily error report for 2026-03-15", ""]
for service, count in sorted(counts.items(), key=lambda pair: (-pair[1], pair[0])):
    share = count / total * 100 if total else 0
    lines.append(f"- {service}: {count} errors ({share:.1f}%)")
lines.extend(["", f"Total: {total}"])
await post_message({"channel": "#ops", "text": "\n".join(lines)})
