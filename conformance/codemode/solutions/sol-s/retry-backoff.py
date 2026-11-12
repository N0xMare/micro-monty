import asyncio

for attempt in range(1, 6):
    try:
        quote = await get_quote({"symbol": "WAYN"})
        break
    except RuntimeError as error:
        if "429" not in str(error) or attempt == 5:
            raise
        await asyncio.sleep(2 ** (attempt - 1))
await submit_answer({"price": quote["price"], "attempts": attempt})
