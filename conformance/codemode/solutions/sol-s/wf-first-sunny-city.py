chosen = None
for city in ("Oslo", "Berlin", "Madrid", "Rome", "Sydney"):
    weather = await get_weather({"city": city})
    if weather["condition"] == "sunny" and weather["temp_c"] >= 15:
        chosen = city
        break
if chosen is None:
    raise ValueError("No city meets the weather criteria")
await submit_answer({"city": chosen})
