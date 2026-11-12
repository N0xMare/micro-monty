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


urls = [
    "https://api.example.com",
    "https://auth.example.com",
    "https://billing.example.com",
    "https://cdn.example.com",
    "https://mail.example.com",
    "https://search.example.com",
    "https://status.example.com",
]
results = await asyncio.gather(*(call(check_health, {"url": u}) for u in urls), return_exceptions=True)

up, unreachable, down = [], [], []
for u, r in zip(urls, results):
    if isinstance(r, BaseException):
        print(f"{u}: {type(r).__name__}: {r}")
        unreachable.append(u)
    elif json.loads(r).get("up"):
        up.append(u)
    else:
        down.append(u)

print("up:", up, "down:", down, "unreachable:", unreachable)
print(await submit_answer({"up": sorted(up), "unreachable": sorted(unreachable)}))
