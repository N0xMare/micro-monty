import asyncio
import json

async def probe(url):
    for attempt in range(4):
        try:
            raw = await check_health({"url": url})
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise
            if attempt == 3:
                return None
        else:
            return json.loads(raw)

urls = ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://cdn.example.com", "https://mail.example.com", "https://search.example.com", "https://status.example.com"]
results = await asyncio.gather(*(probe(url) for url in urls))
await submit_answer({"up": sorted(url for url, r in zip(urls, results) if r is not None and r["up"]), "unreachable": sorted(url for url, r in zip(urls, results) if r is None)})
