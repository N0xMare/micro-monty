import asyncio
async def main():
    return await asyncio.gather(*(asyncio.sleep(0, i) for i in range(3)))
print(asyncio.run(main()))
