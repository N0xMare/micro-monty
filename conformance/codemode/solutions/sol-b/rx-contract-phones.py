import re
text = await read_file({"path": "/docs/contract.txt"})
await submit_answer({"phones": re.findall(r"\+[0-9](?:[0-9 ()-]*[0-9])?", text)})
