import json

chosen = None
for city in ["Oslo", "Berlin", "Madrid", "Rome", "Sydney"]:
    w = json.loads(await get_weather({"city": city}))
    print(city, w["condition"], w["temp_c"])
    if w["condition"].strip().lower() == "sunny" and w["temp_c"] >= 15:
        chosen = city
        break

print(await submit_answer({"city": chosen}))
