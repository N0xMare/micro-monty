import re
text = await read_file({"path": "/docs/contract.txt"})
text = re.sub(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+", "[email]", text)
text = re.sub(r"\+[0-9 ()-]*[0-9]", "[phone]", text)
await write_file({"path": "/out/contract_redacted.txt", "content": text})
