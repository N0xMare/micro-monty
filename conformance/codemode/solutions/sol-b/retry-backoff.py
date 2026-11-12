import asyncio, json

for attempt in range(1, 6):
    try:
        raw = await get_quote({"symbol": "WAYN"})
    except RuntimeError as exc:
        if "429" not in str(exc) or attempt == 5:
            raise
        await asyncio.sleep(2 ** (attempt - 1))
    else:
        quote = json.loads(raw)
        break
await submit_answer({"price": quote["price"], "attempts": attempt})
