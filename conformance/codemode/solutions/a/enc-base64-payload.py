import json
import base64
import re

raw = await read_file({"path": "/inbox/payload.b64"})
s = re.sub(r"\s+", "", raw)
s += "=" * (-len(s) % 4)
try:
    data = base64.b64decode(s, validate=True)
except Exception:
    data = base64.urlsafe_b64decode(s)
doc = json.loads(data.decode("utf-8-sig"))
print(json.dumps(doc, indent=1)[:2000])

def find(d, names):
    if isinstance(d, dict):
        for k, v in d.items():
            if k.lower() in names:
                return v
        for v in d.values():
            r = find(v, names)
            if r is not None:
                return r
    return None

batch = find(doc, {"batch", "batch_id", "batchid", "id"} ) if not isinstance(doc, list) else None
if isinstance(batch, dict):
    batch = batch.get("id", batch)
records = find(doc, {"records", "items", "entries", "rows"})
if records is None and isinstance(doc, list):
    records = doc
total = sum(float(r.get("amount", 0)) for r in records)
print(batch, len(records), total)
print(await submit_answer({"batch": str(batch), "records": len(records), "total": round(total, 2)}))
