import textwrap

paragraph = ("Our quarterly results exceeded expectations across all regions, driven by strong hardware "
             "sales and a growing services business. We thank every customer and partner for their "
             "continued trust.")
body = "\n".join(textwrap.wrap(paragraph, width=50, break_long_words=False, break_on_hyphens=False))
print(body)
assert all(len(l) <= 50 and l == l.rstrip() for l in body.splitlines())

res = await send_email({"to": "ceo@example.com", "subject": "Quarterly note", "body": body})
print(res)
