import asyncio, json

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

urls = [
    "https://api.example.com", "https://auth.example.com", "https://billing.example.com",
    "https://cdn.example.com", "https://mail.example.com", "https://search.example.com",
    "https://status.example.com",
]
results = await asyncio.gather(*[with_retry(check_health, {"url": u}) for u in urls], return_exceptions=True)

up, unreachable, down = [], [], []
for u, r in zip(urls, results):
    if isinstance(r, BaseException):
        print(f"{u}: unreachable ({type(r).__name__}: {r})")
        unreachable.append(u)
        continue
    res = json.loads(r)
    (up if res.get("up") else down).append(u)

print("up:", sorted(up))
print("down (responded, not up):", sorted(down))
print("unreachable:", sorted(unreachable))
print(await submit_answer({"up": sorted(up), "unreachable": sorted(unreachable)}))
