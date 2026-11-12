import asyncio
async def retry_call(function, arguments):
    for attempt in range(4):
        try:
            return await function(arguments)
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                raise
services = await retry_call(list_services, {})
logs = await asyncio.gather(*(retry_call(fetch_logs, {"service": service, "date": "2026-03-15"}) for service in services))
errors = {service: sum(1 for line in text.splitlines() if len(line.split()) > 1 and line.split()[1] == "ERROR") for service, text in zip(services, logs)}
await submit_answer({"errors": errors})
