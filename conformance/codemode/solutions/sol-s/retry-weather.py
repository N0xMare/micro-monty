for attempt in range(4):
    try:
        weather = await get_weather({"city": "Madrid"})
        break
    except (TimeoutError, RuntimeError) as error:
        if not (isinstance(error, TimeoutError) or "503" in str(error)) or attempt == 3:
            raise
await submit_answer({"temp_c": weather["temp_c"]})
