import asyncio
import json

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney"]
weather = [json.loads(r) for r in await asyncio.gather(*(get_weather({"city": city}) for city in cities))]
await submit_answer({"warmest": max(weather, key=lambda w: w["temp_c"])["city"], "coldest": min(weather, key=lambda w: w["temp_c"])["city"], "mean_temp_c": round(sum(w["temp_c"] for w in weather) / len(weather), 1)})
