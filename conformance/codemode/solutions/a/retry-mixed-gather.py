import asyncio, json

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def with_retry(fn, args, retries=3):
    for attempt in range(retries + 1):
        try:
            return await fn(args)
        except Exception as e:
            if not transient(e) or attempt == retries:
                raise
            await asyncio.sleep(0.2 * 2 ** attempt)

accounts = ["A001", "A002", "A003", "A004"]
results = await asyncio.gather(
    *[with_retry(get_transactions, {"account_id": a}) for a in accounts], return_exceptions=True
)

nets, failed = {}, []
for a, r in zip(accounts, results):
    if isinstance(r, BaseException):
        print(f"{a} failed: {type(r).__name__}: {r}")
        failed.append(a)
        continue
    txs = json.loads(r)
    nets[a] = round(sum(t["amount"] for t in txs), 2)

print("nets:", nets, "failed:", failed)
print(await submit_answer({"nets": nets, "failed": failed}))
