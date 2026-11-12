import asyncio, json
from collections import Counter

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def with_retry(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)

accounts = {}
offset = 0
total = None
while True:
    page = json.loads(await with_retry(list_accounts, {"offset": offset, "limit": 10}))
    total = page["total"]
    batch = page["accounts"]
    for a in batch:
        accounts[a["account_id"]] = a
    offset += len(batch)
    if not batch or offset >= total:
        break

counts = dict(Counter(a["status"] for a in accounts.values()))
print(len(accounts), "accounts of total", total, counts)
print(await submit_answer({"counts": counts}))
