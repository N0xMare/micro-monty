import json
from collections import Counter

async def page_at(offset):
    for attempt in range(4):
        try:
            raw = await list_accounts({"offset": offset, "limit": 10})
        except (TimeoutError, RuntimeError) as error:
            if attempt == 3 or not (isinstance(error, TimeoutError) or "503" in str(error)):
                raise
        else:
            return json.loads(raw)

offset = 0
counts = Counter()
while True:
    page = await page_at(offset)
    counts.update(a["status"] for a in page["accounts"])
    offset += len(page["accounts"])
    if offset >= page["total"]:
        break
    if not page["accounts"]:
        raise RuntimeError("Empty page before all accounts were returned")
await submit_answer({"counts": dict(counts)})
