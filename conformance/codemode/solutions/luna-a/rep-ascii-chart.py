import asyncio
from datetime import date, timedelta

services = await list_services({})
dates = [(date(2026, 3, 13) + timedelta(days=offset)).isoformat() for offset in range(3)]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service in services for day in dates))
counts = {day: 0 for day in dates}
for service_index, service in enumerate(services):
    for day_index, day in enumerate(dates):
        log = logs[service_index * len(dates) + day_index]
        counts[day] += sum(1 for line in log.splitlines() if " ERROR " in line)
maximum = max(counts.values(), default=0)
lines = [f"{day} {'#' * (round(counts[day] / maximum * 40) if maximum else 0):<40} {counts[day]}" for day in dates]
await post_message({"channel": "#ops", "text": "\n".join(lines)})
