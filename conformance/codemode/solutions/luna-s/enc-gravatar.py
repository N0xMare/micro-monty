import asyncio
import hashlib

customers = await asyncio.gather(*(get_customer({"customer_id": customer_id}) for customer_id in range(1, 6)))
hashes = {str(customer["id"]): hashlib.md5(customer["email"].strip().lower().encode("utf-8")).hexdigest() for customer in customers}
await submit_answer({"hashes": hashes})
