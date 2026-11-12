import base64
import json
from decimal import Decimal

encoded = await read_file({"path": "/inbox/payload.b64"})
payload = json.loads(base64.b64decode(encoded).decode("utf-8"), parse_float=Decimal)
total = sum((Decimal(str(record["amount"])) for record in payload["records"]), Decimal(0))
await submit_answer({"batch": payload["batch"], "records": len(payload["records"]), "total": float(round(total, 2))})
