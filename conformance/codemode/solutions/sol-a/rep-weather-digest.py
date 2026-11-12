import asyncio
import json

cities = sorted(["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"])
weather = [json.loads(r) for r in await asyncio.gather(*(get_weather({"city": city}) for city in cities))]
lines = [f"{w['city']}: {w['temp_c']:.1f}°C, {w['condition']}" for w in weather]
await post_message({"channel": "#general", "text": "\n".join(lines)})
