import asyncio, json

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney"]
results = [json.loads(r) for r in await asyncio.gather(
    *(get_weather({"city": c}) for c in cities))]
warmest = max(results, key=lambda w: w["temp_c"])["city"]
coldest = min(results, key=lambda w: w["temp_c"])["city"]
mean = round(sum(w["temp_c"] for w in results) / len(results), 1)
print(warmest, coldest, mean)
print(await submit_answer({"warmest": warmest, "coldest": coldest, "mean_temp_c": mean}))
