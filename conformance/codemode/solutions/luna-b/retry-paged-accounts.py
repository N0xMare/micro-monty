import asyncio, json

async def retry_page(offset):
    for attempt in range(4):
        try:
            return json.loads(await list_accounts({"offset": offset, "limit": 10}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise
            await asyncio.sleep(0)

first = await retry_page(0)
pages = [first]
if first["total"] > 10:
    pages.extend(await asyncio.gather(*(retry_page(offset) for offset in range(10, first["total"], 10))))
counts = {}
for page in pages:
    for account in page["accounts"]:
        counts[account["status"]] = counts.get(account["status"], 0) + 1
await submit_answer({"counts": counts})
