from urllib.parse import urlsplit, urlunsplit, parse_qsl, urlencode

src = "https://shop.example.com/products?utm_source=mail&utm_medium=email&id=7&ref=home&utm_campaign=spring&lang=en"
parts = urlsplit(src)
kept = [(k, v) for k, v in parse_qsl(parts.query, keep_blank_values=True) if not k.startswith("utm_")]
url = urlunsplit(parts._replace(query=urlencode(kept)))
print(url)
print(await submit_answer({"url": url}))
