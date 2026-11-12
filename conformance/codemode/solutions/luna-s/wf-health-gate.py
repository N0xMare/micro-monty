urls = ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://search.example.com"]
message = "All clear"
for url in urls:
    result = await check_health({"url": url})
    if not result["up"]:
        message = f"Deploy blocked: {url} is down"
        break
await post_message({"channel": "#ops", "text": message})
