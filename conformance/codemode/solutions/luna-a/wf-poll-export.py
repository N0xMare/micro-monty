import json
import asyncio

job = json.loads(await start_export({"dataset": "orders"}))
state = None
result_url = None
polls = 0
for attempt in range(10):
    if attempt:
        await asyncio.sleep(2)
    status = json.loads(await get_job_status({"job_id": job["job_id"]}))
    polls += 1
    state = status["state"]
    result_url = status.get("result_url")
    if state in ("done", "failed"):
        break
await submit_answer({"state": state, "result_url": result_url, "polls": polls})
