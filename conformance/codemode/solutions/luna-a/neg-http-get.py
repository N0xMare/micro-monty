import json

response = json.loads(await http_get({"url": "https://api.example.com/v1/status"}))
body = json.loads(response["body"])
components = body["components"]
if isinstance(components, dict):
    down = {name: item["uptime_pct"] for name, item in components.items() if item.get("status") != "operational"}
else:
    down = {item["name"]: item["uptime_pct"] for item in components if item.get("status") != "operational"}
await submit_answer({"components": down})
