import json
for url in ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://search.example.com"]:
    health = json.loads(await check_health({"url": url}))
    if not health["up"]:
        await post_message({"channel": "#ops", "text": f"Deploy blocked: {url} is down"})
        break
else:
    await post_message({"channel": "#ops", "text": "All clear"})
