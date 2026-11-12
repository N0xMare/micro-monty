import asyncio
import json

services = json.loads(await list_services({}))
dates = ["2026-03-13", "2026-03-14", "2026-03-15"]
calls = [(service, date) for service in services for date in dates]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": date}) for service, date in calls))
line_count = sum(len(text.splitlines()) for text in logs)
await submit_answer({"lines": line_count})
