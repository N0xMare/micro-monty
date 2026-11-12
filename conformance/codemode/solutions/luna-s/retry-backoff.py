import asyncio
price = None
attempts = 0
for attempts in range(1, 6):
    try:
        quote = await get_quote({"symbol": "WAYN"})
        price = quote["price"]
        break
    except RuntimeError as error:
        if "429 Too Many Requests" not in str(error) or attempts == 5:
            raise
        await asyncio.sleep(2 ** (attempts - 1))
await submit_answer({"price": price, "attempts": attempts})
