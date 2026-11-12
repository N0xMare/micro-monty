import json
for city in ["Oslo", "Berlin", "Madrid", "Rome", "Sydney"]:
    weather = json.loads(await get_weather({"city": city}))
    if weather["condition"] == "sunny" and weather["temp_c"] >= 15:
        await submit_answer({"city": city})
        break
else:
    raise RuntimeError("No qualifying city")
