import asyncio, json

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": date}) for service in services for date in dates))
counts = {date: sum(1 for service_index in range(len(services))
                    for line in logs[service_index * len(dates) + dates.index(date)].splitlines()
                    if " ERROR " in line) for date in dates}
maximum = max(counts.values(), default=0)
lines = [f"{date} {'#' * (round(count / maximum * 40) if maximum else 0):<40} {count}" for date, count in counts.items()]
await post_message({"channel": "#ops", "text": "\n".join(lines)})
