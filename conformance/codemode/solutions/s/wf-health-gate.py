urls = [
    "https://api.example.com",
    "https://auth.example.com",
    "https://billing.example.com",
    "https://search.example.com",
]
down = None
for url in urls:
    h = await check_health({"url": url})
    print(url, h)
    if not h["up"]:
        down = url
        break

text = f"Deploy blocked: {down} is down" if down else "All clear"
await post_message({"channel": "#ops", "text": text})
print("posted:", text)

if "submit_answer" in globals():
    await submit_answer({"blocked": down is not None, "down_url": down, "message": text})
