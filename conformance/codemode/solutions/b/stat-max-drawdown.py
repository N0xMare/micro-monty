import asyncio
import json

ids = [f"A{i:03d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_transactions({"account_id": a}) for a in ids), return_exceptions=True)

lows = {}
for a, r in zip(ids, results):
    if isinstance(r, Exception):
        print(a, "error:", r)
        continue
    running = low = 0.0
    for tx in json.loads(r):
        running += tx["amount"]
        low = min(low, running)
    lows[a] = round(low, 2)

print(lows)
worst = min(lows, key=lows.get)
answer = {"account_id": worst, "low": lows[worst]}
print(answer)
print(await submit_answer(answer))
