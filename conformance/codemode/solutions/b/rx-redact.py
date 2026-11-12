import re

text = await read_file({"path": "/docs/contract.txt"})

email_re = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
phone_re = re.compile(r"\+[0-9 ()\-]*[0-9]")

redacted, n_email = email_re.subn("[email]", text)
redacted, n_phone = phone_re.subn("[phone]", redacted)
print(f"{n_email} emails, {n_phone} phones redacted")

result = await write_file({"path": "/out/contract_redacted.txt", "content": redacted})
print(result)

# The catalog lists no submit_answer tool; call it only if the harness injected one anyway.
if "submit_answer" in globals():
    print(await submit_answer({"path": "/out/contract_redacted.txt"}))
