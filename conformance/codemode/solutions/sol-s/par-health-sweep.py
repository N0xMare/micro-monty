import asyncio

urls = [f"https://{name}.example.com" for name in ("api", "auth", "billing", "cdn", "mail", "search", "status")]
results = await asyncio.gather(*(check_health({"url": url}) for url in urls))
down = sorted(url for url, result in zip(urls, results) if not result["up"])
latencies = [result["latency_ms"] for result in results if result["up"] and result["latency_ms"] is not None]
await submit_answer({"down": down, "avg_latency_ms": round(sum(latencies) / len(latencies), 1)})
