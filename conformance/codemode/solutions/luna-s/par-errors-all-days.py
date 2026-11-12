import asyncio

services = await list_services({})
days = ["2026-03-13", "2026-03-14", "2026-03-15"]
jobs = [(service, day) for service in services for day in days]
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": day}) for service, day in jobs))
errors = {service: 0 for service in services}
for (service, _), text in zip(jobs, logs):
    errors[service] += sum(len(line.split()) > 1 and line.split()[1] == "ERROR" for line in text.splitlines())
await submit_answer({"errors": errors})
