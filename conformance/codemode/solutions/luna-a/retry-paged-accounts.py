import json
from collections import Counter

async def list_with_retries(offset):
    for attempt in range(4):
        try:
            return json.loads(await list_accounts({"offset": offset, "limit": 10}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise

accounts = []
offset = 0
total = None
while total is None or offset < total:
    page = await list_with_retries(offset)
    accounts.extend(page["accounts"])
    total = page["total"]
    offset += len(page["accounts"])
    if not page["accounts"]:
        break
counts = dict(Counter(account["status"] for account in accounts))
await submit_answer({"counts": counts})
