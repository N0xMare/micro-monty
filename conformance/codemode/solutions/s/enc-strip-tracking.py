from urllib.parse import urlsplit, urlunsplit

url = "https://shop.example.com/products?utm_source=mail&utm_medium=email&id=7&ref=home&utm_campaign=spring&lang=en"
parts = urlsplit(url)
kept = [p for p in parts.query.split("&") if p and not p.split("=", 1)[0].lower().startswith("utm_")]
clean = urlunsplit(parts._replace(query="&".join(kept)))
print(clean)
await submit_answer({"url": clean})
