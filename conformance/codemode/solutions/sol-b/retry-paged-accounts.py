import asyncio, json
from collections import Counter

async def page(offset):
    for attempt in range(4):
        try:
            raw = await list_accounts({"offset": offset, "limit": 10})
        except (TimeoutError, RuntimeError) as exc:
            if not (isinstance(exc, TimeoutError) or "503" in str(exc)) or attempt == 3:
                raise
        else:
            return json.loads(raw)

first = await page(0)
rest = await asyncio.gather(*(page(offset) for offset in range(10, first["total"], 10)))
counts = Counter(a["status"] for p in [first] + list(rest) for a in p["accounts"])
await submit_answer({"counts": dict(counts)})
