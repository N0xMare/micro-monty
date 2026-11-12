import json

response = await http_get({"url": "https://api.example.com/v1/status"})
data = json.loads(response["body"])
components = data["components"]
if isinstance(components, dict):
    items = components.items()
else:
    items = ((item.get("name", item.get("component")), item) for item in components)
down = {}
for name, item in items:
    if isinstance(item, dict):
        status = item.get("status")
        uptime = item.get("uptime_pct", item.get("uptime"))
    else:
        status, uptime = item["status"], item.get("uptime_pct", item.get("uptime"))
    if status != "operational":
        down[name] = uptime
await submit_answer({"components": down})
