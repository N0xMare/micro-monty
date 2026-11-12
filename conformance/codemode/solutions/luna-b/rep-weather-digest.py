import asyncio, json

cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"]
weather = [json.loads(raw) for raw in await asyncio.gather(*(get_weather({"city": city}) for city in cities))]
lines = [f"{item['city']}: {item['temp_c']:.1f}°C, {item['condition']}" for item in sorted(weather, key=lambda item: item["city"])]
await post_message({"channel": "#general", "text": "\n".join(lines)})
