import json

for attempt in range(4):
    try:
        raw = await get_weather({"city": "Madrid"})
    except (TimeoutError, RuntimeError) as error:
        if attempt == 3 or not (isinstance(error, TimeoutError) or "503" in str(error)):
            raise
    else:
        weather = json.loads(raw)
        break
await submit_answer({"temp_c": weather["temp_c"]})
