import asyncio, json

job_id = json.loads(await start_export({"dataset": "orders"}))["job_id"]
status, polls = None, 0
while polls < 10:
    status = json.loads(await get_job_status({"job_id": job_id}))
    polls += 1
    print(polls, status)
    if status["state"] in ("done", "failed"):
        break
    if polls < 10:
        await asyncio.sleep(2)

result = {"state": status["state"], "result_url": status.get("result_url"), "polls": polls}
print(result)
print(await submit_answer(result))
