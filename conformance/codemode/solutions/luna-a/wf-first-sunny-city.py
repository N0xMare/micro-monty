import json

chosen = None
for city in ["Oslo", "Berlin", "Madrid", "Rome", "Sydney"]:
    weather = json.loads(await get_weather({"city": city}))
    if weather["condition"].lower() == "sunny" and weather["temp_c"] >= 15:
        chosen = city
        break
await submit_answer({"city": chosen})
