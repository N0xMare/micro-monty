import asyncio
import json

services = json.loads(await list_services({}))
days = ["2026-03-13", "2026-03-14", "2026-03-15"]
results = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service in services for day in days))
line_count = 0
for service, day, text in zip((s for s in services for _ in days), (d for _ in services for d in days), results):
    print(f"=== {service} {day} ===")
    print(text, end="" if not text or text.endswith("\n") else "\n")
    line_count += len(text.splitlines())
await submit_answer({"lines": line_count})
