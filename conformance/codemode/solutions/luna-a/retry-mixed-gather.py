import asyncio
import json

async def transactions_with_retries(account_id):
    for attempt in range(4):
        try:
            return await get_transactions({"account_id": account_id})
        except (TimeoutError, RuntimeError) as error:
            transient = isinstance(error, TimeoutError) or "503" in str(error)
            if not transient or attempt == 3:
                raise

accounts = ["A001", "A002", "A003", "A004"]
results = await asyncio.gather(*(transactions_with_retries(account) for account in accounts), return_exceptions=True)
nets, failed = {}, []
for account, result in zip(accounts, results):
    if isinstance(result, BaseException):
        failed.append(account)
    else:
        transactions = json.loads(result)
        nets[account] = round(sum(transaction["amount"] for transaction in transactions), 2)
await submit_answer({"nets": nets, "failed": failed})
