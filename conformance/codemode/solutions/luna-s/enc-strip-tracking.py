from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

url = "https://shop.example.com/products?utm_source=mail&utm_medium=email&id=7&ref=home&utm_campaign=spring&lang=en"
parts = urlsplit(url)
query = [(key, value) for key, value in parse_qsl(parts.query, keep_blank_values=True) if not key.startswith("utm_")]
cleaned = urlunsplit((parts.scheme, parts.netloc, parts.path, urlencode(query), parts.fragment))
await submit_answer({"url": cleaned})
