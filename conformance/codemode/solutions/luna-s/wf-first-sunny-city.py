city_found = ""
for city in ("Oslo", "Berlin", "Madrid", "Rome", "Sydney"):
    weather = await get_weather({"city": city})
    if weather["condition"].lower() == "sunny" and weather["temp_c"] >= 15:
        city_found = city
        break
await submit_answer({"city": city_found})
