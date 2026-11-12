import asyncio

services = await list_services({})
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-15"}) for s in services))
counts = {s: sum(len(p := line.split()) > 1 and p[1] == "ERROR" for line in log.splitlines()) for s, log in zip(services, logs)}
total = sum(counts.values())
lines = ["Daily error report for 2026-03-15", ""]
for s in sorted(counts, key=lambda s: (-counts[s], s)):
    share = counts[s] / total * 100 if total else 0
    lines.append(f"- {s}: {counts[s]} errors ({share:.1f}%)")
lines.extend(["", f"Total: {total}"])
await post_message({"channel": "#ops", "text": "\n".join(lines)})
