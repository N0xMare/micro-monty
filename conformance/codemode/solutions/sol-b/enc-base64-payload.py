import base64, json
from decimal import Decimal
encoded = await read_file({"path": "/inbox/payload.b64"})
document = json.loads(base64.b64decode(encoded).decode("utf-8"), parse_float=Decimal)
await submit_answer({"batch": document.get("batch", document.get("batch_id")), "records": len(document["records"]), "total": float(round(sum((Decimal(str(r["amount"])) for r in document["records"]), Decimal(0)), 2))})
