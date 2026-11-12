from urllib.parse import urlsplit, urlunsplit, parse_qsl, urlencode

url = urlsplit("https://shop.example.com/products?utm_source=mail&utm_medium=email&id=7&ref=home&utm_campaign=spring&lang=en")
query = urlencode([(key, value) for key, value in parse_qsl(url.query, keep_blank_values=True) if not key.startswith("utm_")])
await submit_answer({"url": urlunsplit((url.scheme, url.netloc, url.path, query, url.fragment))})
