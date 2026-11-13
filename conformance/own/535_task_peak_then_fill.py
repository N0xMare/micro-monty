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


# Work after the peak that costs fuel but holds little memory: most parks
# fall here, with a small heap, and few in the fill below, whose images
# are near the limit (with the fill nearly all of the fuel, the release
# gate's 1:1000 run took ~1,000 parks of 100-250 MB images and passed its
# 1,800 s limit).
zeros = 0
block = bytes(1 << 20)
for _ in range(80):
    zeros += block.count(0)
block = None


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
print(total_, len(text), junk, zeros, counts[0] > 0)
