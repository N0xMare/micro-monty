import asyncio, json
from collections import Counter

first = json.loads(await list_customers({"page": 1}))
pages = [first]
total_pages = first.get("total_pages", 1)
if total_pages > 1:
    rest = await asyncio.gather(*(list_customers({"page": p}) for p in range(2, total_pages + 1)))
    pages += [json.loads(r) for r in rest]

countries = Counter(c["country"] for pg in pages for c in pg["customers"])
countries = dict(countries)
print(sum(countries.values()), countries)
print(await submit_answer({"countries": countries}))
