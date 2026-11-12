import asyncio

async def fetch_with_retries(service):
    for attempt in range(4):
        try:
            return await fetch_logs({"service": service, "date": "2026-03-15"})
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise

services = await list_services({})
logs = await asyncio.gather(*(fetch_with_retries(service) for service in services))
errors = {service: sum(1 for line in log.splitlines() if " ERROR " in line) for service, log in zip(services, logs)}
await submit_answer({"errors": errors})
