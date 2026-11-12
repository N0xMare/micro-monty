import asyncio, json

async def retry(tool, args):
    for attempt in range(4):
        try:
            return await tool(args)
        except (TimeoutError, RuntimeError) as exc:
            if not (isinstance(exc, TimeoutError) or "503" in str(exc)) or attempt == 3:
                raise

services = json.loads(await retry(list_services, {}))
logs = await asyncio.gather(*(retry(fetch_logs, {"service": s, "date": "2026-03-15"}) for s in services))
errors = {s: sum(len(parts := line.split()) > 1 and parts[1] == "ERROR" for line in log.splitlines()) for s, log in zip(services, logs)}
await submit_answer({"errors": errors})
