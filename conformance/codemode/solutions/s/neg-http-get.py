import json

resp = await http_get({"url": "https://api.example.com/v1/status"})
print("status:", resp["status"], resp["content_type"])
if resp["status"] != 200:
    raise RuntimeError(f"status API returned {resp['status']}: {resp['body'][:500]}")
data = json.loads(resp["body"])

comps = data.get("components", data) if isinstance(data, dict) else data
if isinstance(comps, dict):
    comps = [{"name": k, **v} if isinstance(v, dict) else {"name": k, "status": v}
             for k, v in comps.items()]

def uptime(c):
    for k in ("uptime_pct", "uptime", "uptime_percent", "uptimePct"):
        if k in c:
            return c[k]
    return None

components = {}
for c in comps:
    status = str(c.get("status", "")).lower()
    print(c.get("name"), status, uptime(c))
    if status != "operational":
        components[c.get("name") or c.get("id")] = uptime(c)
print(components)
print(await submit_answer({"components": components}))
