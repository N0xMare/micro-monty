deploys = await list_deployments({"service": "billing"})
deploys = [d for d in deploys if d["service"] == "billing"]
if not deploys:
    raise RuntimeError("no billing deployments found")
last = max(deploys, key=lambda d: d["started_at"])
print(last)

if last["status"] != "success":
    inc = await create_incident({
        "title": f"billing: last deploy {last['status']}",
        "severity": "sev3",
        "services": ["billing"],
        "summary": f"Deployment {last['id']} by {last['author']} at {last['started_at']} ended with status {last['status']}.",
    })
    print(inc)
else:
    await submit_answer({"status": "ok"})
