import asyncio
import json

ids = [4, 77, 12, 300, 25]
results = await asyncio.gather(*(get_customer({"customer_id": i}) for i in ids), return_exceptions=True)
found = {}
not_found = []
for customer_id, result in zip(ids, results):
    if isinstance(result, LookupError):
        not_found.append(customer_id)
    elif isinstance(result, BaseException):
        raise result
    else:
        found[str(customer_id)] = json.loads(result)["name"]
await submit_answer({"found": found, "not_found": not_found})
