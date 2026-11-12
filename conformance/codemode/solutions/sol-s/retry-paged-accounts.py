import asyncio
from collections import Counter

async def fetch(offset):
    for attempt in range(4):
        try:
            return await list_accounts({"offset": offset, "limit": 10})
        except (TimeoutError, RuntimeError) as error:
            if not (isinstance(error, TimeoutError) or "503" in str(error)) or attempt == 3:
                raise

first = await fetch(0)
rest = await asyncio.gather(*(fetch(offset) for offset in range(10, first["total"], 10)))
counts = Counter(a["status"] for page in [first, *rest] for a in page["accounts"])
await submit_answer({"counts": dict(counts)})
