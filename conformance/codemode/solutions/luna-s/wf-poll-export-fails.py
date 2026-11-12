import asyncio

job = await start_export({"dataset": "orders"})
state = "running"
result_url = None
polls = 0
while polls < 10 and state not in ("done", "failed"):
    if polls:
        await asyncio.sleep(2)
    status = await get_job_status({"job_id": job["job_id"]})
    polls += 1
    state = status["state"]
    result_url = status["result_url"]
await submit_answer({"state": state, "result_url": result_url, "polls": polls})
