import json

resp = json.loads(await http_get({"url": "https://shop.example.com/robots.txt"}))
body = resp["body"]

disallowed = []
sitemap = None
agents = []          # user agents of the current group
in_rules = False     # whether the current group has started its rules

for raw in body.splitlines():
    line = raw.split("#", 1)[0].strip()
    if not line or ":" not in line:
        continue
    key, value = line.split(":", 1)
    key, value = key.strip().lower(), value.strip()
    if key == "user-agent":
        if in_rules:
            agents = []
            in_rules = False
        agents.append(value)
    elif key in ("disallow", "allow", "crawl-delay"):
        in_rules = True
        if key == "disallow" and "*" in agents and value:
            disallowed.append(value)
    elif key == "sitemap":
        if sitemap is None:
            sitemap = value

answer = {"disallowed": disallowed, "sitemap": sitemap}
print(answer)
print(await submit_answer(answer))
