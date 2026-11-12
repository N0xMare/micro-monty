import json

text = json.loads(await http_get({"url": "https://shop.example.com/robots.txt"}))["body"]
active = False
disallowed = []
sitemap = ""
for raw in text.splitlines():
    line = raw.split("#", 1)[0].strip()
    if not line:
        continue
    key, _, value = line.partition(":")
    key, value = key.strip().lower(), value.strip()
    if key == "user-agent":
        active = value == "*"
    elif key == "disallow" and active:
        disallowed.append(value)
    elif key == "sitemap":
        sitemap = value
await submit_answer({"disallowed": disallowed, "sitemap": sitemap})
