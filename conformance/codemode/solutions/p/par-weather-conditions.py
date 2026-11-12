import asyncio
async def f(c):
    return await get_weather({"city": c})
t = asyncio.create_task(f("Oslo"))
print(await t)
