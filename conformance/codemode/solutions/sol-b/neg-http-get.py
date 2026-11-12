import json

response = json.loads(await http_get({"url": "https://api.example.com/v1/status"}))
data = json.loads(response["body"])
components = {component["name"]: component["uptime_pct"] for component in data["components"] if component["status"] != "operational"}
await submit_answer({"components": components})
