import asyncio

urls = sorted(f"https://{host}.example.com" for host in ["api", "auth", "billing", "cdn", "mail", "search", "status"])
results = await asyncio.gather(*(check_health({"url": u}) for u in urls))
lines = []
for url, r in zip(urls, results):
    host = url.removeprefix("https://")
    lines.append(f"✅ {host} {r['latency_ms']} ms" if r["up"] else f"❌ {host} DOWN")
await post_message({"channel": "#status", "text": "\n".join(lines)})
