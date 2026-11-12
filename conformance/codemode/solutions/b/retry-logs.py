import asyncio
import json


def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))


async def call(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)


services = json.loads(await call(list_services, {}))
logs = await asyncio.gather(*(call(fetch_logs, {"service": s, "date": "2026-03-15"}) for s in services))

errors = {}
for s, text in zip(services, logs):
    n = 0
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1] == "ERROR":
            n += 1
    errors[s] = n

print(errors)
print(await submit_answer({"errors": errors}))
