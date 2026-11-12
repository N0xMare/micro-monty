import asyncio
urls = ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://cdn.example.com", "https://mail.example.com", "https://search.example.com", "https://status.example.com"]
async def probe(url):
    for attempt in range(4):
        try:
            return await check_health({"url": url})
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                return None
results = await asyncio.gather(*(probe(url) for url in urls))
up = sorted(item["url"] for item in results if item is not None and item["up"])
unreachable = sorted(url for url, item in zip(urls, results) if item is None or not item["up"])
await submit_answer({"up": up, "unreachable": unreachable})
