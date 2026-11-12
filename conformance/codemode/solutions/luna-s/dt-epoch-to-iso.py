import datetime

deployments = await list_deployments({})
failed = [
    {"id": deployment["id"], "service": deployment["service"], "started": datetime.datetime.fromtimestamp(int(deployment["started_at"]), datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
    for deployment in deployments if deployment["status"] == "failed"
]
await submit_answer({"failed": failed})
