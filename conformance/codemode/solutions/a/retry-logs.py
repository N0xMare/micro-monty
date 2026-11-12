import asyncio, json, re

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def with_retry(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)

services = json.loads(await with_retry(list_services, {}))
logs = await asyncio.gather(*[with_retry(fetch_logs, {"service": s, "date": "2026-03-15"}) for s in services])

line_re = re.compile(r"^\S+\s+ERROR\s")
errors = {}
for s, text in zip(services, logs):
    errors[s] = sum(1 for line in text.splitlines() if line_re.match(line))

print(errors)
print(await submit_answer({"errors": errors}))
