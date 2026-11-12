import asyncio
import json

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
jobs = [(date, service) for date in dates for service in services]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": date}) for date, service in jobs))
counts = dict.fromkeys(dates, 0)
for (date, service), log in zip(jobs, logs):
    counts[date] += sum(len(parts := line.split()) >= 2 and parts[1] == "ERROR" for line in log.splitlines())
maximum = max(counts.values())
lines = []
for date in dates:
    count = counts[date]
    bar = "#" * (round(count / maximum * 40) if maximum else 0)
    lines.append(f"{date} {bar:<40} {count}")
await post_message({"channel": "#ops", "text": "\n".join(lines)})
