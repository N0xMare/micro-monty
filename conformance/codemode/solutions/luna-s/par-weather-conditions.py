import asyncio
cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"]
weather = await asyncio.gather(*(get_weather({"city": city}) for city in cities))
counts = {}
for item in weather:
    counts[item["condition"]] = counts.get(item["condition"], 0) + 1
await submit_answer({"conditions": counts})
