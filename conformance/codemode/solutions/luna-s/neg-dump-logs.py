import asyncio

services = await list_services({})
days = ["2026-03-13", "2026-03-14", "2026-03-15"]
jobs = [(service, day) for service in services for day in days]
logs = await asyncio.gather(*(fetch_logs({"service": s, "date": d}) for s, d in jobs))
line_count = 0
for (service, day), log_text in zip(jobs, logs):
    print(f"--- {service} {day} ---")
    print(log_text, end="" if not log_text or log_text.endswith("\n") else "\n")
    line_count += len(log_text.splitlines())
await submit_answer({"lines": line_count})
