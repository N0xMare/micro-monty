import json
import re

text = await read_file({"path": "/docs/contract.txt"})

email_re = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
phone_re = re.compile(r"\+[0-9() -]*[0-9]")

# Emails first, so a "+" inside an address local part is not taken for a phone.
redacted = email_re.sub("[email]", text)
redacted = phone_re.sub("[phone]", redacted)

print(len(email_re.findall(text)), "emails,", len(phone_re.findall(email_re.sub("", text))), "phones")
result = json.loads(await write_file({"path": "/out/contract_redacted.txt", "content": redacted}))
print(result)
