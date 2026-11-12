import json

chosen = None
for city in ["Oslo", "Berlin", "Madrid", "Rome", "Sydney"]:
    w = json.loads(await get_weather({"city": city}))
    if w["condition"].strip().lower() == "sunny" and w["temp_c"] >= 15:
        chosen = city
        break

await submit_answer({"city": chosen})
