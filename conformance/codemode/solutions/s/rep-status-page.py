import asyncio

urls = sorted([
    "https://api.example.com", "https://auth.example.com", "https://billing.example.com",
    "https://cdn.example.com", "https://mail.example.com", "https://search.example.com",
    "https://status.example.com",
])
results = await asyncio.gather(*(check_health({"url": u}) for u in urls))

lines = []
for u, r in zip(urls, results):
    host = u.removeprefix("https://")
    if r["up"]:
        lines.append(f"✅ {host} {r['latency_ms']} ms")
    else:
        lines.append(f"❌ {host} DOWN")
text = "\n".join(lines)
print(text)

res = await post_message({"channel": "#status", "text": text})
print(res)
