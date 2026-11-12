import asyncio, re

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def call(fn, args):
    for attempt in range(4):  # 1 try + 3 retries
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            await asyncio.sleep(0.5 * 2 ** attempt)

services = await call(list_services, {})
logs = await asyncio.gather(*(call(fetch_logs, {"service": s, "date": "2026-03-15"}) for s in services))

pat = re.compile(r"^\S+\s+ERROR\b")
errors = {}
for s, text in zip(services, logs):
    errors[s] = sum(1 for line in text.splitlines() if pat.match(line))

print(errors)
print(await submit_answer({"errors": errors}))
