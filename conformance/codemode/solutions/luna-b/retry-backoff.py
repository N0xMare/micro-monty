import asyncio, json

attempts = 0
for attempt in range(1, 6):
    attempts = attempt
    try:
        quote = json.loads(await get_quote({"symbol": "WAYN"}))
        break
    except RuntimeError as error:
        if "429 Too Many Requests" not in str(error) or attempt == 5:
            raise
        await asyncio.sleep(2 ** (attempt - 1))
await submit_answer({"price": quote["price"], "attempts": attempts})
