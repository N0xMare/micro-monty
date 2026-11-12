import asyncio, json
from collections import Counter

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"]
weather = await asyncio.gather(*(get_weather({"city": city}) for city in cities))
conditions = Counter(json.loads(result)["condition"] for result in weather)
await submit_answer({"conditions": dict(conditions)})
