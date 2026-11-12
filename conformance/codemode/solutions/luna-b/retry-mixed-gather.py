import asyncio, json

accounts = ["A001", "A002", "A003", "A004"]

async def fetch(account):
    for attempt in range(4):
        try:
            return json.loads(await get_transactions({"account_id": account}))
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise
            await asyncio.sleep(0)

results = await asyncio.gather(*(fetch(account) for account in accounts), return_exceptions=True)
nets, failed = {}, []
for account, result in zip(accounts, results):
    if isinstance(result, BaseException):
        failed.append(account)
    else:
        nets[account] = round(sum(item["amount"] for item in result), 2)
await submit_answer({"nets": nets, "failed": failed})
