chosen = None
for city in ["Oslo", "Berlin", "Madrid", "Rome", "Sydney"]:
    w = await get_weather({"city": city})
    print(city, w["condition"], w["temp_c"])
    if w["condition"].strip().lower() == "sunny" and w["temp_c"] >= 15:
        chosen = city
        break

if chosen is None:
    raise RuntimeError("no city is sunny with >= 15 C")
await submit_answer({"city": chosen})
