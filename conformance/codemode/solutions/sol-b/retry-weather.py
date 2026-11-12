import json

for attempt in range(4):
    try:
        raw = await get_weather({"city": "Madrid"})
    except (TimeoutError, RuntimeError) as exc:
        if not (isinstance(exc, TimeoutError) or "503" in str(exc)) or attempt == 3:
            raise
    else:
        weather = json.loads(raw)
        break
await submit_answer({"temp_c": weather["temp_c"]})
