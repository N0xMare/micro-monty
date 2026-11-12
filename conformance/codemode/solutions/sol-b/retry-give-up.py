import asyncio, json

async def probe(url):
    for attempt in range(4):
        try:
            raw = await check_health({"url": url})
        except (TimeoutError, RuntimeError) as exc:
            if not (isinstance(exc, TimeoutError) or "503" in str(exc)):
                raise
            if attempt == 3:
                return None
        else:
            return json.loads(raw)

urls = [f"https://{h}.example.com" for h in ["api", "auth", "billing", "cdn", "mail", "search", "status"]]
results = await asyncio.gather(*(probe(u) for u in urls))
await submit_answer({"up": sorted(u for u, r in zip(urls, results) if r is not None and r["up"]), "unreachable": sorted(u for u, r in zip(urls, results) if r is None)})
