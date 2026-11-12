import asyncio
async def fetch_account(account):
    for attempt in range(4):
        try:
            transactions = await get_transactions({"account_id": account})
            return account, round(sum(item["amount"] for item in transactions), 2), None
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                return account, None, error
results = await asyncio.gather(*(fetch_account(account) for account in ["A001", "A002", "A003", "A004"]))
nets = {account: net for account, net, error in results if error is None}
failed = [account for account, net, error in results if error is not None]
await submit_answer({"nets": nets, "failed": failed})
