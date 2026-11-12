import asyncio

accounts = ["A001", "A002", "A003", "A004"]

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and "503" in str(e))

async def fetch(acc):
    for attempt in range(4):  # 1 try + 3 retries
        try:
            return await get_transactions({"account_id": acc})
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            await asyncio.sleep(0.5 * 2 ** attempt)

results = await asyncio.gather(*(fetch(a) for a in accounts), return_exceptions=True)

nets, failed = {}, []
for a, r in zip(accounts, results):
    if isinstance(r, BaseException):
        print(f"{a} failed: {type(r).__name__}: {r}")
        failed.append(a)
    else:
        nets[a] = round(sum(t["amount"] for t in r), 2)

print(nets, failed)
print(await submit_answer({"nets": nets, "failed": failed}))
