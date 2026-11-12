import asyncio
import json

cities = ["London", "Atlantis", "Tokyo", "Gotham", "Austin", "Chicago"]
results = await asyncio.gather(*(get_weather({"city": city}) for city in cities), return_exceptions=True)
temps = {}
failed = []
for city, result in zip(cities, results):
    if isinstance(result, BaseException):
        failed.append(city)
    else:
        temps[city] = json.loads(result)["temp_c"]
await submit_answer({"temps": temps, "failed": failed})
