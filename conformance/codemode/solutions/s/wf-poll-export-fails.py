import asyncio

job = await start_export({"dataset": "orders"})
job_id = job["job_id"]

polls = 0
status = None
while polls < 10:
    status = await get_job_status({"job_id": job_id})
    polls += 1
    print(polls, status)
    if status["state"] in ("done", "failed"):
        break
    if polls < 10:
        await asyncio.sleep(2)

state = status["state"] if status else "unknown"
await submit_answer({
    "state": state,
    "result_url": status.get("result_url") if status else None,
    "polls": polls,
})
