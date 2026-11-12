import asyncio
import json

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney"]
results = await asyncio.gather(*(get_weather({"city": city}) for city in cities))
weather = [json.loads(result) for result in results]
warmest = max(weather, key=lambda item: item["temp_c"])
coldest = min(weather, key=lambda item: item["temp_c"])
mean_temp_c = round(sum(item["temp_c"] for item in weather) / len(weather), 1)
await submit_answer({"warmest": warmest["city"], "coldest": coldest["city"], "mean_temp_c": mean_temp_c})
