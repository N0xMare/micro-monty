import asyncio

urls = [
    "https://api.example.com",
    "https://auth.example.com",
    "https://billing.example.com",
    "https://cdn.example.com",
    "https://mail.example.com",
    "https://search.example.com",
    "https://status.example.com",
]

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def probe(url):
    for attempt in range(4):  # 1 try + 3 retries
        try:
            return await check_health({"url": url})
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            await asyncio.sleep(0.5 * 2 ** attempt)

results = await asyncio.gather(*(probe(u) for u in urls), return_exceptions=True)

up, unreachable, down = [], [], []
for u, r in zip(urls, results):
    if isinstance(r, BaseException):
        print(f"{u}: unreachable ({type(r).__name__}: {r})")
        unreachable.append(u)
    elif r.get("up"):
        up.append(u)
    else:
        down.append(u)

print("up:", sorted(up))
print("down (responded, not up):", sorted(down))
print("unreachable:", sorted(unreachable))
print(await submit_answer({"up": sorted(up), "unreachable": sorted(unreachable)}))
