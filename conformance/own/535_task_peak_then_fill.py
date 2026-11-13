# A peak of 20,000 finished tasks, then memory filled: a session restored
# after the peak must have the memory the session that went on has
# (restore-vs-snapshot; the task table's capacity is charged, step 4).
import asyncio


async def one(i):
    await asyncio.sleep(0)
    return i


async def main():
    return sum(await asyncio.gather(*[one(i) for i in range(20000)]))


total_ = asyncio.run(main())
text = ''.join(str(i) for i in range(5000))
junk = 0
for i in range(300000):
    junk += i


# Fill up to 320 MiB, or the 256 MiB memory limit if that is lower. The
# counts depend on the memory the session has left, so they stay in
# globals for the final-state digest and are not printed. A park this
# near the limit is refused on the target that charges more (`cross`,
# step 4).
CAP = 320 << 20
counts = [0, 0, 0]
keep = []
total = 0
for k, size in enumerate((1 << 20, 1 << 16, 1 << 10)):
    try:
        while total < CAP:
            keep.append(bytearray(size))
            counts[k] += 1
            total += size
    except MemoryError:
        pass
keep = None
print(total_, len(text), junk, counts[0] > 0)
