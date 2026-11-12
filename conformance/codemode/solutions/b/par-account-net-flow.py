import asyncio, json

ids = [f"A{n:03d}" for n in range(1, 13)]
results = await asyncio.gather(*(get_transactions({"account_id": a}) for a in ids))
net = {a: round(sum(t["amount"] for t in json.loads(r)), 2) for a, r in zip(ids, results)}
negative = {a: v for a, v in net.items() if v < 0}
print(net)
print(await submit_answer({"negative": negative}))
