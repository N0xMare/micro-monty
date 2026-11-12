from urllib.parse import urlsplit, urlunsplit

url = "https://shop.example.com/products?utm_source=mail&utm_medium=email&id=7&ref=home&utm_campaign=spring&lang=en"
p = urlsplit(url)
kept = [kv for kv in p.query.split("&") if kv and not kv.split("=", 1)[0].startswith("utm_")]
clean = urlunsplit((p.scheme, p.netloc, p.path, "&".join(kept), p.fragment))
print(clean)
print(await submit_answer({"url": clean}))
