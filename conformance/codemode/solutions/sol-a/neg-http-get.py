import json

response = json.loads(await http_get({"url": "https://api.example.com/v1/status"}))
if not 200 <= response["status"] < 300:
    raise RuntimeError(f"Status API returned HTTP {response['status']}")
body = json.loads(response["body"])
components = {component["name"]: component["uptime_pct"] for component in body["components"] if component["status"] != "operational"}
await submit_answer({"components": components})
