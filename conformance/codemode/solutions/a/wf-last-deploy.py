import json

deps = [d for d in json.loads(await list_deployments({"service": "billing"})) if d["service"] == "billing"]
if not deps:
    raise ValueError("No billing deployments found")

last = max(deps, key=lambda d: d["started_at"])
if last["status"] != "success":
    inc = json.loads(await create_incident({
        "title": f"billing: last deploy {last['status']}",
        "severity": "sev3",
        "services": ["billing"],
        "summary": f"Most recent billing deployment {last['id']} by {last['author']} (started_at {last['started_at']}) ended with status {last['status']}.",
    }))
    print(json.dumps(inc))
else:
    await submit_answer({"status": "ok"})
