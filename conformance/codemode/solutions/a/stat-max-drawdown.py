import asyncio
import json

ids = [f"A{i:03d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_transactions({"account_id": a}) for a in ids))

lows = {}
for a, raw in zip(ids, results):
    running = 0.0
    low = 0.0
    for tx in json.loads(raw):
        running += tx["amount"]
        low = min(low, running)
    lows[a] = round(low, 2)

print(lows)
worst = min(ids, key=lambda a: lows[a])
answer = {"account_id": worst, "low": lows[worst]}
print(answer)
print(await submit_answer(answer))
