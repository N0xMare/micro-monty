import re

text = await read_file({"path": "/docs/contract.txt"})
text = re.sub(r"\+\d(?:[\d ()-]*\d)?", "[phone]", text)
text = re.sub(r"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}", "[email]", text, flags=re.I)
await write_file({"path": "/out/contract_redacted.txt", "content": text})
