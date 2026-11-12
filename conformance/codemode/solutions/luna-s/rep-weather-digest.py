import asyncio
cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney", "New York", "Chicago", "Austin", "Toronto"]
weather = await asyncio.gather(*(get_weather({"city": c}) for c in cities))
lines = [f"{x['city']}: {x['temp_c']:.1f}°C, {x['condition']}" for x in sorted(weather, key=lambda x: x["city"])]
await post_message({"channel": "#general", "text": "\n".join(lines)})
