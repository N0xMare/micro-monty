import asyncio
import re
from html.parser import HTMLParser

ids = list(range(1, 6))
resps = await asyncio.gather(
    *[http_get({"url": f"https://shop.example.com/products?id={i}"}) for i in ids],
    return_exceptions=True,
)

def parse_price(s):
    if s is None:
        return None
    m = re.search(r'\d[\d,]*(?:\.\d+)?|\.\d+', s)
    return float(m.group(0).replace(",", "")) if m else None

class ProductParser(HTMLParser):
    VOID = {"meta", "link", "img", "br", "hr", "input", "source", "area", "base", "col", "embed", "wbr"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.h1_depth = 0
        self.h1_parts = None
        self.title = None
        self.stack = []  # (tag, is_price, is_old)
        self.attr_price = None
        self.price_texts = []
        self.cur_price = None

    def _old(self):
        return any(old for _, _, old in self.stack)

    def handle_starttag(self, tag, attrs):
        tag = tag.lower()
        a = {k.lower(): (v or "") for k, v in attrs}
        cls = a.get("class", "").lower()
        is_old = tag in ("del", "s", "strike") or any(w in cls for w in ("old", "was", "strike", "original", "compare", "regular"))
        is_price = a.get("itemprop", "").lower() == "price" or "price" in cls.split() or any(c.startswith("price") or c.endswith("price") for c in cls.split())
        if self.attr_price is None and not is_old and not self._old():
            if a.get("itemprop", "").lower() == "price" and a.get("content"):
                self.attr_price = a["content"]
            elif a.get("data-price"):
                self.attr_price = a["data-price"]
        if tag in self.VOID:
            return
        if tag == "h1" and self.title is None:
            self.h1_depth += 1
            if self.h1_parts is None:
                self.h1_parts = []
        self.stack.append((tag, is_price, is_old))
        if is_price and not self._old() and self.cur_price is None:
            self.cur_price = (len(self.stack), [])

    def handle_startendtag(self, tag, attrs):
        a = {k.lower(): (v or "") for k, v in attrs}
        if self.attr_price is None and not self._old():
            if a.get("itemprop", "").lower() == "price" and a.get("content"):
                self.attr_price = a["content"]
            elif a.get("data-price"):
                self.attr_price = a["data-price"]

    def handle_endtag(self, tag):
        tag = tag.lower()
        if tag in self.VOID:
            return
        # pop to matching tag
        for idx in range(len(self.stack) - 1, -1, -1):
            if self.stack[idx][0] == tag:
                if self.cur_price is not None and self.cur_price[0] > idx:
                    self.price_texts.append("".join(self.cur_price[1]))
                    self.cur_price = None
                del self.stack[idx:]
                break
        if tag == "h1" and self.h1_depth > 0:
            self.h1_depth -= 1
            if self.h1_depth == 0 and self.title is None:
                self.title = " ".join("".join(self.h1_parts).split())

    def handle_data(self, data):
        if self.h1_depth > 0 and self.h1_parts is not None:
            self.h1_parts.append(data)
        if self.cur_price is not None and not any(old for _, _, old in self.stack[self.cur_price[0]:]):
            self.cur_price[1].append(data)

products = []
for i, r in zip(ids, resps):
    if isinstance(r, Exception):
        print(i, "error:", r)
        continue
    if r["status"] != 200:
        print(i, "status", r["status"])
        continue
    body = r["body"]
    p = ProductParser()
    p.feed(body)
    p.close()
    price = parse_price(p.attr_price)
    if price is None:
        for t in p.price_texts:
            price = parse_price(t)
            if price is not None:
                break
    if price is None:
        # fallback: first currency amount outside <del>/<s>
        cleaned = re.sub(r'<(del|s|strike)\b.*?</\1>', ' ', body, flags=re.S | re.I)
        cleaned = re.sub(r'<!--.*?-->', ' ', cleaned, flags=re.S)
        m = re.search(r'[$€£]\s?(\d[\d,]*(?:\.\d+)?)', cleaned)
        price = float(m.group(1).replace(",", "")) if m else None
    print(i, repr(p.title), price, p.attr_price, p.price_texts)
    products.append({"id": i, "title": p.title, "price": price})

products.sort(key=lambda x: x["id"])
await submit_answer({"products": products})
