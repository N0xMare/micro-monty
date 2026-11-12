import asyncio, json

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney"]
results = [json.loads(s) for s in await asyncio.gather(*(get_weather({"city": c}) for c in cities))]
temps = {c: w["temp_c"] for c, w in zip(cities, results)}

warmest = max(temps, key=temps.get)
coldest = min(temps, key=temps.get)
mean = round(sum(temps.values()) / len(temps), 1)

print(temps, warmest, coldest, mean)
print(await submit_answer({"warmest": warmest, "coldest": coldest, "mean_temp_c": mean}))
