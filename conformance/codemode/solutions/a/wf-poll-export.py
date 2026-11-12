import asyncio, json

job_id = json.loads(await start_export({"dataset": "orders"}))["job_id"]

polls, status = 0, None
while polls < 10:
    if polls:
        await asyncio.sleep(2)
    status = json.loads(await get_job_status({"job_id": job_id}))
    polls += 1
    if status["state"] in ("done", "failed"):
        break

await submit_answer({
    "state": status["state"],
    "result_url": status.get("result_url"),
    "polls": polls,
})
