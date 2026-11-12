from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

url = "https://shop.example.com/products?utm_source=mail&utm_medium=email&id=7&ref=home&utm_campaign=spring&lang=en"
parts = urlsplit(url)
query = urlencode([(k, v) for k, v in parse_qsl(parts.query, keep_blank_values=True) if not k.startswith("utm_")])
await submit_answer({"url": urlunsplit((parts.scheme, parts.netloc, parts.path, query, parts.fragment))})
