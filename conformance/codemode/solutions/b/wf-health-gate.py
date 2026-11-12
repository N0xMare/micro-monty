import json

urls = ["https://api.example.com", "https://auth.example.com", "https://billing.example.com", "https://search.example.com"]
down = None
for url in urls:
    h = json.loads(await check_health({"url": url}))
    print(url, h)
    if not h["up"]:
        down = url
        break

text = f"Deploy blocked: {down} is down" if down else "All clear"
print(await post_message({"channel": "#ops", "text": text}))
print({"posted": text, "down": down})
# submit_answer is not in this cell's tool catalog, so the result is reported via print only.
