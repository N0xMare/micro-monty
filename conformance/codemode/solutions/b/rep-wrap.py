import textwrap

paragraph = ("Our quarterly results exceeded expectations across all regions, driven by strong hardware "
             "sales and a growing services business. We thank every customer and partner for their "
             "continued trust.")
body = "\n".join(textwrap.wrap(paragraph, width=50, break_long_words=False, break_on_hyphens=False))

print(body)
print(await send_email({"to": "ceo@example.com", "subject": "Quarterly note", "body": body}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"body": body}))
