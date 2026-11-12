import asyncio
import json

urls = sorted(["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://cdn.example.com", "https://mail.example.com", "https://search.example.com", "https://status.example.com"])
results = [json.loads(r) for r in await asyncio.gather(*(check_health({"url": u}) for u in urls))]
lines = []
for url, result in zip(urls, results):
    host = url.removeprefix("https://")
    lines.append(f"✅ {host} {result['latency_ms']} ms" if result["up"] else f"❌ {host} DOWN")
await post_message({"channel": "#status", "text": "\n".join(lines)})
