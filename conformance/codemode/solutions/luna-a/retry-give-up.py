import asyncio
import json

async def probe_with_retries(url):
    for attempt in range(4):
        try:
            return json.loads(await check_health({"url": url}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                return None

urls = ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://cdn.example.com", "https://mail.example.com", "https://search.example.com", "https://status.example.com"]
results = await asyncio.gather(*(probe_with_retries(url) for url in urls))
up = sorted(item["url"] for item in results if item is not None and item["up"])
unreachable = sorted(item["url"] for item in results if item is None or not item["up"])
await submit_answer({"up": up, "unreachable": unreachable})
