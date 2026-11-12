resp = await http_get({"url": "https://shop.example.com/robots.txt"})
print(resp["status"])
body = resp["body"]

disallowed = []
sitemaps = []
group_agents = []
in_rules = False  # whether the current group has started its rule lines

for raw in body.splitlines():
    line = raw.split("#", 1)[0].strip()
    if not line or ":" not in line:
        continue
    field, value = line.split(":", 1)
    field = field.strip().lower()
    value = value.strip()
    if field == "user-agent":
        if in_rules:
            group_agents = []
            in_rules = False
        group_agents.append(value)
    elif field == "sitemap":
        sitemaps.append(value)
    elif field in ("disallow", "allow", "crawl-delay"):
        in_rules = True
        if field == "disallow" and "*" in group_agents and value:
            disallowed.append(value)

print(disallowed, sitemaps)
await submit_answer({"disallowed": disallowed, "sitemap": sitemaps[0] if sitemaps else ""})
