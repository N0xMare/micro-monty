import asyncio
import json

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"]
results = await asyncio.gather(*(get_weather({"city": city}) for city in cities))
weather = sorted((json.loads(result) for result in results), key=lambda item: item["city"])
text = "\n".join(f"{item['city']}: {item['temp_c']:.1f}°C, {item['condition']}" for item in weather)
await post_message({"channel": "#general", "text": text})
