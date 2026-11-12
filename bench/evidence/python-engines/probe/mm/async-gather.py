import asyncio
async def f(i): return i
async def m(): return await asyncio.gather(*[f(i) for i in range(3)])
r = asyncio.run(m())
print(r)
