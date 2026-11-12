import asyncio
async def list_page(offset):
    for attempt in range(4):
        try:
            return await list_accounts({"offset": offset, "limit": 10})
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                raise
first = await list_page(0)
accounts = list(first["accounts"])
offsets = list(range(10, first["total"], 10))
if offsets:
    pages = await asyncio.gather(*(list_page(offset) for offset in offsets))
    for page in pages:
        accounts.extend(page["accounts"])
counts = {}
for account in accounts:
    counts[account["status"]] = counts.get(account["status"], 0) + 1
await submit_answer({"counts": counts})
