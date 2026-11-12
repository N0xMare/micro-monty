import asyncio, json

urls = sorted(f"https://{host}.example.com" for host in ["api", "auth", "billing", "cdn", "mail", "search", "status"])
results = [json.loads(s) for s in await asyncio.gather(*(check_health({"url": u}) for u in urls))]
lines = []
for url, result in zip(urls, results):
    host = url.removeprefix("https://")
    lines.append(f"✅ {host} {result['latency_ms']} ms" if result["up"] else f"❌ {host} DOWN")
await post_message({"channel": "#status", "text": "\n".join(lines)})
