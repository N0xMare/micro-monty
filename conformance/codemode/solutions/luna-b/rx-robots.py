import json

page = json.loads(await http_get({"url": "https://shop.example.com/robots.txt"}))
disallowed, sitemap, in_star = [], None, False
for raw in page["body"].splitlines():
    line = raw.split("#", 1)[0].strip()
    if not line: continue
    key, _, value = line.partition(":")
    key, value = key.strip().lower(), value.strip()
    if key == "user-agent": in_star = value == "*"
    elif key == "disallow" and in_star: disallowed.append(value)
    elif key == "sitemap": sitemap = value
await submit_answer({"disallowed": disallowed, "sitemap": sitemap})
