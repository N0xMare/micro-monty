import asyncio
import json
from collections import Counter

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"]
weather = [json.loads(r) for r in await asyncio.gather(*(get_weather({"city": city}) for city in cities))]
await submit_answer({"conditions": dict(Counter(w["condition"] for w in weather))})
