import json
import asyncio

accounts = [f"A{i:03d}" for i in range(1, 13)]
results = await asyncio.gather(*(get_transactions({"account_id": a}) for a in accounts))
net = {a: round(sum(t["amount"] for t in json.loads(r)), 2) for a, r in zip(accounts, results)}
print(net)
negative = {a: v for a, v in net.items() if v < 0}
await submit_answer({"negative": negative})
