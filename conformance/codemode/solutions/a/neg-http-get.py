import json

resp = json.loads(await http_get({"url": "https://api.example.com/v1/status"}))
print("status:", resp["status"], "content_type:", resp["content_type"])
if resp["status"] != 200:
    raise RuntimeError(f"status API returned {resp['status']}: {resp['body'][:500]}")
data = json.loads(resp["body"])
print(json.dumps(data, indent=2)[:3000])

comps = data.get("components", data) if isinstance(data, dict) else data
if isinstance(comps, dict):  # name -> {status, uptime_pct}
    comps = [{"name": k, **v} for k, v in comps.items()]
components = {}
for c in comps:
    if str(c.get("status", "")).lower() != "operational":
        components[c["name"]] = c.get("uptime_pct", c.get("uptime"))
print(components)
print(await submit_answer({"components": components}))
