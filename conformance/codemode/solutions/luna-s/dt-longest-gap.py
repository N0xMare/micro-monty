deployments = await list_deployments({})
earlier, later = max(zip(deployments, deployments[1:]), key=lambda pair: int(pair[1]["started_at"]) - int(pair[0]["started_at"]))
gap_hours = (int(later["started_at"]) - int(earlier["started_at"])) / 3600
await submit_answer({"hours": round(gap_hours, 2), "before": earlier["id"], "after": later["id"]})
