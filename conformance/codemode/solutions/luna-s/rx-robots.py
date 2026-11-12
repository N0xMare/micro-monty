import re

response = await http_get({"url": "https://shop.example.com/robots.txt"})
agents = []
disallowed = []
sitemaps = []
for raw in response["body"].splitlines():
    line = raw.split("#", 1)[0].strip()
    if not line or ":" not in line: continue
    key, value = (part.strip() for part in line.split(":", 1))
    key = key.lower()
    if key == "user-agent":
        agents.append(value.lower())
    elif key == "disallow" and "*" in agents and value:
        disallowed.append(value)
    elif key == "sitemap":
        sitemaps.append(value)
await submit_answer({"disallowed": disallowed, "sitemap": sitemaps[0] if sitemaps else ""})
