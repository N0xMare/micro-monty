import asyncio
cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney"]
weather = await asyncio.gather(*(get_weather({"city": city}) for city in cities))
await submit_answer({"warmest": max(weather, key=lambda x: x["temp_c"])["city"], "coldest": min(weather, key=lambda x: x["temp_c"])["city"], "mean_temp_c": round(sum(x["temp_c"] for x in weather) / len(weather), 1)})
