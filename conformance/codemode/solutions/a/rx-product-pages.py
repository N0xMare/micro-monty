import asyncio
import json
import re
from html.parser import HTMLParser


class ProductParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.in_h1 = False
        self.h1 = None
        self._h1_buf = []
        self.price_texts = []
        self._price_depth = 0
        self._price_buf = []
        self.meta_price = None
        self._stack = []

    def handle_starttag(self, tag, attrs):
        a = {k.lower(): (v or "") for k, v in attrs}
        tag = tag.lower()
        if tag == "h1" and self.h1 is None:
            self.in_h1 = True
            self._h1_buf = []
        if tag == "meta" and a.get("itemprop") == "price" or a.get("property") in ("product:price:amount", "og:price:amount"):
            if self.meta_price is None and a.get("content"):
                self.meta_price = a["content"]
        if tag in ("meta", "br", "img", "input", "link", "hr"):
            return
        is_price = (
            "price" in a.get("class", "").lower().split()
            or a.get("itemprop") == "price"
            or "price" in a.get("id", "").lower()
            or "data-price" in a
        )
        if is_price and "data-price" in a and a["data-price"]:
            self.price_texts.append(a["data-price"])
        if self._price_depth:
            self._price_depth += 1
        elif is_price:
            self._price_depth = 1
            self._price_buf = []

    def handle_endtag(self, tag):
        tag = tag.lower()
        if tag == "h1" and self.in_h1:
            self.in_h1 = False
            self.h1 = " ".join("".join(self._h1_buf).split())
        if self._price_depth:
            self._price_depth -= 1
            if self._price_depth == 0:
                self.price_texts.append("".join(self._price_buf))

    def handle_data(self, data):
        if self.in_h1:
            self._h1_buf.append(data)
        if self._price_depth:
            self._price_buf.append(data)


num_re = re.compile(r"[0-9][0-9.,]*")


def to_price(s):
    m = num_re.search(s or "")
    if not m:
        return None
    v = m.group(0).rstrip(".,")
    if "," in v and "." in v:
        if v.rfind(",") > v.rfind("."):
            v = v.replace(".", "").replace(",", ".")
        else:
            v = v.replace(",", "")
    elif "," in v:
        head, _, tail = v.rpartition(",")
        v = v.replace(",", "") if len(tail) == 3 else head.replace(",", "") + "." + tail
    try:
        return float(v)
    except ValueError:
        return None


async def fetch(i):
    return i, json.loads(await http_get({"url": f"https://shop.example.com/products?id={i}"}))


results = await asyncio.gather(*(fetch(i) for i in range(1, 6)))

products = []
for i, resp in sorted(results):
    if resp.get("status") != 200:
        print("page", i, "status", resp.get("status"))
        continue
    p = ProductParser()
    p.feed(resp["body"])
    p.close()
    price = None
    for cand in [p.meta_price, *p.price_texts]:
        price = to_price(cand)
        if price is not None:
            break
    if price is None:
        m = re.search(r"(?:\$|USD|€|£)\s?([0-9][0-9,]*(?:\.[0-9]+)?)", resp["body"])
        if m:
            price = to_price(m.group(1))
    products.append({"id": i, "title": p.h1, "price": price})

print(products)
print(await submit_answer({"products": products}))
