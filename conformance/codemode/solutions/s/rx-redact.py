import re

text = await read_file({"path": "/docs/contract.txt"})

email_re = re.compile(r'[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}')
phone_re = re.compile(r'\+[0-9 ()\-]*[0-9]')

# Emails first, so a "+" inside an email's local part is never taken for a phone.
redacted, n_emails = email_re.subn("[email]", text)
redacted, n_phones = phone_re.subn("[phone]", redacted)
print("emails:", n_emails, "phones:", n_phones)

result = await write_file({"path": "/out/contract_redacted.txt", "content": redacted})
print(result)
