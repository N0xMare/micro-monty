from urllib.parse import urlencode

params = [("q", 'laptop pro 14"'), ("category", "hardware & more"), ("sort", "price"), ("page", 2)]
url = "https://shop.example.com/search?" + urlencode(params)
print(url)
await submit_answer({"url": url})
