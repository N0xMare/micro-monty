response = await http_get({"url": "https://shop.example.com/robots.txt"})
agents = []
rules_started = False
disallowed = []
sitemap = None
for raw in response["body"].splitlines():
    line = raw.split("#", 1)[0].strip()
    if ":" not in line:
        continue
    key, value = (part.strip() for part in line.split(":", 1))
    key = key.lower()
    if key == "sitemap":
        sitemap = value
    elif key == "user-agent":
        if rules_started:
            agents = []
            rules_started = False
        agents.append(value)
    else:
        rules_started = True
        if key == "disallow" and "*" in agents and value:
            disallowed.append(value)
await submit_answer({"disallowed": disallowed, "sitemap": sitemap})
