import asyncio
async def get_city_weather(city):
    for attempt in range(4):
        try:
            return await get_weather({"city": city})
        except (TimeoutError, RuntimeError) as error:
            if isinstance(error, RuntimeError) and "503" not in str(error):
                raise
            if attempt == 3:
                raise
result = await get_city_weather("Madrid")
await submit_answer({"temp_c": result["temp_c"]})
