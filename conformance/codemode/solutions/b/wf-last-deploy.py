import json

deps = json.loads(await list_deployments({"service": "billing"}))
deps = [d for d in deps if d["service"] == "billing"]
last = max(deps, key=lambda d: d["started_at"])
print("last billing deploy:", last)

if last["status"] != "success":
    inc = json.loads(await create_incident({
        "title": f"billing: last deploy {last['status']}",
        "severity": "sev3",
        "services": ["billing"],
        "summary": f"Deployment {last['id']} by {last['author']} at {last['started_at']} ended with status {last['status']}.",
    }))
    print("incident opened:", inc)
    # submit_answer is documented for the no-incident case only, so it is not called here.
else:
    print(await submit_answer({"status": "ok"}))
