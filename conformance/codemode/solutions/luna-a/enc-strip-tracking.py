from urllib.parse import urlsplit, urlunsplit, parse_qsl, urlencode
url = "https://shop.example.com/products?utm_source=mail&utm_medium=email&id=7&ref=home&utm_campaign=spring&lang=en"
p = urlsplit(url)
query = [(k, v) for k, v in parse_qsl(p.query, keep_blank_values=True) if not k.startswith("utm_")]
await submit_answer({"url": urlunsplit((p.scheme, p.netloc, p.path, urlencode(query), p.fragment))})
