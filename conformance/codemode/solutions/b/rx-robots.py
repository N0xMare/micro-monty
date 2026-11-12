import json

resp = json.loads(await http_get({"url": "https://shop.example.com/robots.txt"}))
print("status", resp["status"])

groups = []          # list of (agents, rules)
agents, rules = [], []
last_was_agent = False
sitemap = None
for raw in resp["body"].splitlines():
    line = raw.split("#", 1)[0].strip()
    if not line or ":" not in line:
        continue
    key, value = line.split(":", 1)
    key, value = key.strip().lower(), value.strip()
    if key == "user-agent":
        if not last_was_agent and agents:
            groups.append((agents, rules))
            agents, rules = [], []
        agents.append(value)
        last_was_agent = True
        continue
    last_was_agent = False
    if key == "sitemap":
        sitemap = sitemap or value
    elif key in ("disallow", "allow"):
        rules.append((key, value))
if agents:
    groups.append((agents, rules))

disallowed = []
for ag, rs in groups:
    if "*" in ag:
        disallowed += [v for k, v in rs if k == "disallow" and v and v not in disallowed]

print(disallowed, sitemap)
print(await submit_answer({"disallowed": disallowed, "sitemap": sitemap}))
