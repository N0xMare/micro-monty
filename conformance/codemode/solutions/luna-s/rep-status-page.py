import asyncio
urls = ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://cdn.example.com", "https://mail.example.com", "https://search.example.com", "https://status.example.com"]
results = await asyncio.gather(*(check_health({"url": url}) for url in urls))
lines = []
for result in sorted(results, key=lambda x: x["url"]):
    host = result["url"].removeprefix("https://")
    lines.append(f"✅ {host} {result['latency_ms']} ms" if result["up"] else f"❌ {host} DOWN")
await post_message({"channel": "#status", "text": "\n".join(lines)})
