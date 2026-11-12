import asyncio
import json

attempts = 0
while True:
    attempts += 1
    try:
        quote = json.loads(await get_quote({"symbol": "WAYN"}))
        break
    except RuntimeError as error:
        if "429 Too Many Requests" not in str(error) or attempts >= 5:
            raise
        await asyncio.sleep(2 ** (attempts - 1))
await submit_answer({"price": quote["price"], "attempts": attempts})
