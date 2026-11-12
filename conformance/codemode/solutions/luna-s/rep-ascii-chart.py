import asyncio
services = await list_services({})
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
results = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s in services for d in dates))
counts = {d: 0 for d in dates}
for i, log in enumerate(results):
    date = dates[i % 3]
    counts[date] += sum(1 for line in log.splitlines() if len(line.split()) > 1 and line.split()[1] == "ERROR")
maximum = max(counts.values(), default=0)
lines = []
for date in dates:
    count = counts[date]
    n = round(count / maximum * 40) if maximum else 0
    lines.append(f"{date} {'#' * n:<40} {count}")
await post_message({"channel": "#ops", "text": "\n".join(lines)})
