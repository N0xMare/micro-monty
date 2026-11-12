import base64
import json
from decimal import Decimal
encoded = await read_file({"path": "/inbox/payload.b64"})
doc = json.loads(base64.b64decode(encoded.strip()))
total = sum((Decimal(str(r["amount"])) for r in doc["records"]), Decimal(0))
await submit_answer({"batch": doc["batch"], "records": len(doc["records"]), "total": float(total.quantize(Decimal("0.01")))})
