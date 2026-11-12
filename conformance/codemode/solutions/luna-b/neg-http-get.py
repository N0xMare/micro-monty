import json

response = json.loads(await http_get({"url": "https://api.example.com/v1/status"}))
body = json.loads(response["body"])
components = {name: component["uptime_pct"] for name, component in body["components"].items() if component["status"] != "operational"}
await submit_answer({"components": components})
