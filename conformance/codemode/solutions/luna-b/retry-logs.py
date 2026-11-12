import asyncio, json

async def retry_fetch(service):
    for attempt in range(4):
        try:
            return await fetch_logs({"service": service, "date": "2026-03-15"})
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise
            await asyncio.sleep(0)

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(retry_fetch(service) for service in services))
errors = {service: sum(" ERROR " in line for line in log.splitlines()) for service, log in zip(services, logs)}
await submit_answer({"errors": errors})
