import json

resp = json.loads(await http_get({"url": "https://api.example.com/v1/status"}))
print("status:", resp["status"], resp["content_type"])
data = json.loads(resp["body"])
comps = data.get("components", data) if isinstance(data, dict) else data
if isinstance(comps, dict):
    comps = [dict(v, name=k) if isinstance(v, dict) else {"name": k, "status": v} for k, v in comps.items()]
components = {}
for c in comps:
    if c.get("status") != "operational":
        components[c["name"]] = c.get("uptime_pct", c.get("uptime"))
print(components)
print(await submit_answer({"components": components}))
