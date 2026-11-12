import asyncio
import json
import re
from collections import Counter

services = json.loads(await list_services({}))
logs = await asyncio.gather(*(fetch_logs({"service": service, "date": date}) for service in services for date in ("2026-03-13", "2026-03-14", "2026-03-15")))
counts = Counter()
for log in logs:
    counts.update(re.findall(r"upstream timeout calling (\S+) after \d+ms", log))
await submit_answer({"timeouts": dict(counts)})
