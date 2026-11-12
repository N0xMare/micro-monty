import asyncio
import json
from collections import Counter

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"]
results = await asyncio.gather(*(get_weather({"city": city}) for city in cities))
conditions = dict(Counter(json.loads(result)["condition"] for result in results))
await submit_answer({"conditions": conditions})
