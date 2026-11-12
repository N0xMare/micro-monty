import asyncio

async def retry(tool, args):
    for attempt in range(4):
        try:
            return await tool(args)
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)) or attempt == 3:
                raise

services = await retry(list_services, {})
logs = await asyncio.gather(*(retry(fetch_logs, {"service": s, "date": "2026-03-15"}) for s in services))
errors = {s: sum(len(p := line.split()) > 1 and p[1] == "ERROR" for line in log.splitlines()) for s, log in zip(services, logs)}
await submit_answer({"errors": errors})
