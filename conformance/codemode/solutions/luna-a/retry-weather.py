import json

for attempt in range(4):
    try:
        weather = json.loads(await get_weather({"city": "Madrid"}))
        break
    except (TimeoutError, RuntimeError) as error:
        transient = isinstance(error, TimeoutError) or "503" in str(error)
        if not transient or attempt == 3:
            raise
await submit_answer({"temp_c": weather["temp_c"]})
