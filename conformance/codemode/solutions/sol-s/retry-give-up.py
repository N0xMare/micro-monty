import asyncio

async def probe(url):
    for attempt in range(4):
        try:
            return await check_health({"url": url})
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise
            if attempt == 3:
                return None

urls = sorted(f"https://{h}.example.com" for h in ["api", "auth", "billing", "cdn", "mail", "search", "status"])
results = await asyncio.gather(*(probe(u) for u in urls))
await submit_answer({"up": [u for u, r in zip(urls, results) if r is not None and r["up"]], "unreachable": [u for u, r in zip(urls, results) if r is None]})
