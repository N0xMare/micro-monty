import asyncio
import json
job = json.loads(await start_export({"dataset": "orders"}))
for polls in range(1, 11):
    if polls > 1:
        await asyncio.sleep(2)
    status = json.loads(await get_job_status({"job_id": job["job_id"]}))
    if status["state"] in ("done", "failed"):
        break
await submit_answer({"state": status["state"], "result_url": status["result_url"], "polls": polls})
