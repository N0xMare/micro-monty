import textwrap
body = "Our quarterly results exceeded expectations across all regions, driven by strong hardware sales and a growing services business. We thank every customer and partner for their continued trust."
await send_email({"to": "ceo@example.com", "subject": "Quarterly note", "body": textwrap.fill(body, width=50, break_long_words=False, break_on_hyphens=False)})
