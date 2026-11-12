import asyncio, json

urls = sorted([
    "https://api.example.com", "https://auth.example.com", "https://billing.example.com",
    "https://cdn.example.com", "https://mail.example.com", "https://search.example.com",
    "https://status.example.com",
])
results = [json.loads(s) for s in await asyncio.gather(*(check_health({"url": u}) for u in urls))]

lines = []
for u, r in zip(urls, results):
    host = u.removeprefix("https://")
    if r["up"]:
        lines.append(f"✅ {host} {r['latency_ms']} ms")
    else:
        lines.append(f"❌ {host} DOWN")
text = "\n".join(lines)

print(text)
print(await post_message({"channel": "#status", "text": text}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"text": text}))
