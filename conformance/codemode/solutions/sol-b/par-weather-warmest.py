import asyncio, json

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney"]
weather = [json.loads(s) for s in await asyncio.gather(*(get_weather({"city": c}) for c in cities))]
await submit_answer({"warmest": max(weather, key=lambda w: w["temp_c"])["city"], "coldest": min(weather, key=lambda w: w["temp_c"])["city"], "mean_temp_c": round(sum(w["temp_c"] for w in weather) / len(weather), 1)})
