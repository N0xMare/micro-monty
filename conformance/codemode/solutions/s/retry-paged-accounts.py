import asyncio
from collections import Counter

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def page(offset):
    for attempt in range(4):  # 1 try + 3 retries
        try:
            return await list_accounts({"offset": offset, "limit": 10})
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            await asyncio.sleep(0.5 * 2 ** attempt)

first = await page(0)
total = first["total"]
rest = await asyncio.gather(*(page(o) for o in range(10, total, 10)))

by_id = {}
for p in [first, *rest]:
    for a in p["accounts"]:
        by_id[a["account_id"]] = a

if len(by_id) != total:
    print(f"WARNING: got {len(by_id)} unique accounts, expected {total}")

counts = dict(Counter(a["status"] for a in by_id.values()))
print(counts)
print(await submit_answer({"counts": counts}))
