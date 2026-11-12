import asyncio
import json

job = json.loads(await start_export({"dataset": "orders"}))
polls = 0; status = None
while polls < 10:
    if polls: await asyncio.sleep(2)
    status = json.loads(await get_job_status({"job_id": job["job_id"]}))
    polls += 1
    if status["state"] in ("done", "failed"): break
await submit_answer({"state": status["state"], "result_url": status["result_url"], "polls": polls})
