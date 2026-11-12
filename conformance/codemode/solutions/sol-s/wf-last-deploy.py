deployments = await list_deployments({"service": "billing"})
latest = max(deployments, key=lambda deployment: deployment["started_at"])
if latest["status"] != "success":
    await create_incident({"title": f"billing: last deploy {latest['status']}", "severity": "sev3", "services": ["billing"]})
else:
    await submit_answer({"status": "ok"})
