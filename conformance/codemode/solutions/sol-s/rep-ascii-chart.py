import asyncio

services = await list_services({})
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
jobs = [(d, s) for d in dates for s in services]
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for d, s in jobs))
counts = dict.fromkeys(dates, 0)
for (d, s), log in zip(jobs, logs):
    counts[d] += sum(len(parts := line.split()) > 1 and parts[1] == "ERROR" for line in log.splitlines())
maximum = max(counts.values())
lines = []
for d in dates:
    bar = "#" * (round(counts[d] / maximum * 40) if maximum else 0)
    lines.append(f"{d} {bar:<40} {counts[d]}")
await post_message({"channel": "#ops", "text": "\n".join(lines)})
