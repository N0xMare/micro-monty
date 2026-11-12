import asyncio
import json
from collections import Counter


def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))


async def call(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)


first = json.loads(await call(list_accounts, {"offset": 0, "limit": 10}))
total = first["total"]
pages = [first]
rest = await asyncio.gather(
    *(call(list_accounts, {"offset": off, "limit": 10}) for off in range(10, total, 10))
)
pages += [json.loads(r) for r in rest]

accounts = {a["account_id"]: a for p in pages for a in p["accounts"]}
if len(accounts) != total:
    print(f"warning: got {len(accounts)} accounts, total says {total}")
counts = dict(Counter(a["status"] for a in accounts.values()))
print(counts)
print(await submit_answer({"counts": counts}))
