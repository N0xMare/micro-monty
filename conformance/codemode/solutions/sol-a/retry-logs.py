import asyncio
import json

async def retry(tool, args):
    for attempt in range(4):
        try:
            return await tool(args)
        except (TimeoutError, RuntimeError) as error:
            if attempt == 3 or not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise

services = json.loads(await retry(list_services, {}))
logs = await asyncio.gather(*(retry(fetch_logs, {"service": s, "date": "2026-03-15"}) for s in services))
counts = {s: sum(len(parts := line.split()) >= 2 and parts[1] == "ERROR" for line in log.splitlines()) for s, log in zip(services, logs)}
await submit_answer({"errors": counts})
