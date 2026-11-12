import json

urls = [
    "https://api.example.com",
    "https://auth.example.com",
    "https://billing.example.com",
    "https://search.example.com",
]

down = None
for url in urls:
    h = json.loads(await check_health({"url": url}))
    if not h["up"]:
        down = url
        break

text_msg = f"Deploy blocked: {down} is down" if down else "All clear"
await post_message({"channel": "#ops", "text": text_msg})
print(json.dumps({"down": down, "message": text_msg}))
if "submit_answer" in globals():
    await submit_answer({"down": down, "message": text_msg})
