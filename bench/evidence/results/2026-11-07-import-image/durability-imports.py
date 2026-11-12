import json
import re, collections
from collections import OrderedDict, namedtuple, deque
import asyncio
p = re.compile(r"(\w+)=(\d+)")
P = namedtuple("P", "a b")
d = OrderedDict(x=P(1, 2))
q = deque([1, 2, 3], maxlen=2)
async def main():
    q2 = asyncio.PriorityQueue()
    await q2.put((2, json.dumps({"k": [1, 2]})))
    await q2.put((1, "first"))
    async with asyncio.TaskGroup() as tg:
        t = tg.create_task(asyncio.sleep(0, "slept"))
    return (await q2.get(), await q2.get(), t.result())
r = asyncio.run(main())
import re as re2, json as j2, asyncio.queues
print(re2 is re, j2 is json, asyncio.queues.Queue is asyncio.Queue)
print(p.findall("a=1 b=22"), d, q, r, re.I | re.M)
print(collections.Counter("abcab").most_common(2), json.loads('{"z": [1.5, null]}'))
try:
    json.loads("[1,")
except json.JSONDecodeError as e:
    print(type(e).__name__, e.pos)
print(sorted(k for k in dir(re) if not k.startswith("_"))[:8])
