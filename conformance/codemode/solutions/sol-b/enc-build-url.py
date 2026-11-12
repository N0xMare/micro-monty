from urllib.parse import urlencode
query = urlencode([("q", 'laptop pro 14"'), ("category", "hardware & more"), ("sort", "price"), ("page", 2)])
await submit_answer({"url": "https://shop.example.com/search?" + query})
