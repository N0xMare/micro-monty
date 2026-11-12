import asyncio
import json

urls = ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://cdn.example.com", "https://mail.example.com", "https://search.example.com", "https://status.example.com"]
results = await asyncio.gather(*(check_health({"url": url}) for url in urls))
health = sorted((json.loads(result) for result in results), key=lambda item: item["url"])
lines = [
    f"{'✅' if item['up'] else '❌'} {item['url'].removeprefix('https://')} {str(item['latency_ms']) + ' ms' if item['up'] else 'DOWN'}"
    for item in health
]
await post_message({"channel": "#status", "text": "\n".join(lines)})
