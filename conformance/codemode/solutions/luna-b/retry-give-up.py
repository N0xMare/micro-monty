import asyncio, json

urls = ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://cdn.example.com", "https://mail.example.com", "https://search.example.com", "https://status.example.com"]

async def probe(url):
    for attempt in range(4):
        try:
            return json.loads(await check_health({"url": url}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise
            await asyncio.sleep(0)

results = await asyncio.gather(*(probe(url) for url in urls), return_exceptions=True)
up, unreachable = [], []
for url, result in zip(urls, results):
    if isinstance(result, BaseException):
        unreachable.append(url)
    elif result["up"]:
        up.append(url)
await submit_answer({"up": sorted(up), "unreachable": sorted(unreachable)})
