"""Text extraction with regular expressions: documents, HTML, XML and logs."""

import datetime as dt
import email.utils
import html
import re
import urllib.parse
from collections import Counter

from ptc import Transcript, r2, task
from tasks.worlds import ACCESS_LOG, FILES, PAGES, PRODUCTS

C = 'regex-extraction'
contract = FILES['/docs/contract.txt']
changelog = FILES['/docs/CHANGELOG.md']

task('rx-contract-emails', C, """
Extract every distinct email address from /docs/contract.txt.
""", ['read_file'],
    answer={'emails': ('array', 'Distinct addresses sorted ascending')},
    expect={'emails': sorted(set(re.findall(r'[\w.+-]+@[\w-]+(?:\.[\w-]+)+', contract)))})

PHONE = r'\+[\d ()-]*\d'
task('rx-contract-phones', C, """
Extract the phone numbers from /docs/contract.txt. A phone number starts with "+" followed by digits,
spaces, parentheses and hyphens, and ends with a digit.
""", ['read_file'],
    answer={'phones': ('array', 'Phone numbers exactly as written, in order of appearance')},
    expect={'phones': re.findall(PHONE, contract)})

task('rx-invoice-ids', C, """
List the invoice ids (format INV-YYYY-NNNN) mentioned in /docs/contract.txt.
""", ['read_file'],
    answer={'invoices': ('array', 'Ids in order of appearance')},
    expect={'invoices': re.findall(r'INV-\d{4}-\d{4}', contract)})

task('rx-contract-terms', C, """
From /docs/contract.txt extract the quarterly fee, the monthly late fee percentage, the payment term in
days, and the term start and end dates.
""", ['read_file'],
    answer={'quarterly_fee_usd': ('number', 'e.g. 1000.0'), 'late_fee_pct': ('number', 'e.g. 2.0'),
            'payment_days': ('integer', 'Days'), 'start': 'YYYY-MM-DD', 'end': 'YYYY-MM-DD'},
    expect={'quarterly_fee_usd': 12500.0, 'late_fee_pct': 1.5, 'payment_days': 30, 'start': '2026-04-01',
            'end': '2027-03-31'})

dates = []
for m in re.finditer(r'(\d{4}-\d{2}-\d{2})|([A-Z][a-z]+ \d{1,2}, \d{4})', contract):
    dates.append(m.group(1) or dt.datetime.strptime(m.group(2), '%B %d, %Y').date().isoformat())
task('rx-contract-dates', C, """
Find every date in /docs/contract.txt, whether written as YYYY-MM-DD or like "March 2, 2026", and
normalize them to YYYY-MM-DD.
""", ['read_file'],
    answer={'dates': ('array', 'Normalized dates in order of appearance')},
    expect={'dates': dates})

rel = [{'version': v, 'date': d} for v, d in re.findall(r'^## \[([\d.]+)\] - (\d{4}-\d{2}-\d{2})', changelog, re.M)]
task('rx-changelog-releases', C, """
List the releases in /docs/CHANGELOG.md (headings like "## [2.4.1] - 2026-03-10").
""", ['read_file'],
    answer={'releases': ('array', 'List of {"version": str, "date": "YYYY-MM-DD"} in file order')},
    expect={'releases': rel})

fixed = []
for sec in re.split(r'^### ', changelog, flags=re.M)[1:]:
    if sec.startswith('Fixed'):
        fixed += [int(n) for n in re.findall(r'#(\d+)', sec.split('\n## ')[0])]
task('rx-changelog-fixed', C, """
Collect the issue numbers (written like "(#412)") listed under every "### Fixed" section of
/docs/CHANGELOG.md.
""", ['read_file'],
    answer={'issues': ('array', 'Issue numbers as integers, ascending')},
    expect={'issues': sorted(fixed)})

home = PAGES['https://shop.example.com/'][2]
links = sorted({urllib.parse.urljoin('https://shop.example.com/', html.unescape(h))
                for h in re.findall(r'href="([^"]+)"', home) if not h.startswith('mailto:')})
task('rx-html-links', C, """
Fetch https://shop.example.com/ and list the distinct link targets (href attributes), resolved to
absolute URLs against the page URL, with HTML entities decoded. Skip mailto: links.
""", ['http_get'],
    answer={'links': ('array', 'Absolute URLs, distinct, sorted ascending')},
    expect={'links': links})

task('rx-product-pages', C, """
Fetch the product pages https://shop.example.com/products?id=1 to id=5 and extract the product title
(the h1) and price.
""", ['http_get'],
    answer={'products': ('array', 'List of {"id": int page id, "title": str, "price": float}, by id')},
    expect={'products': [{'id': i, 'title': PRODUCTS[i - 1]['name'], 'price': PRODUCTS[i - 1]['price']} for i in range(1, 6)]})

sm = PAGES['https://shop.example.com/sitemap.xml'][2]
recent = [int(u) for u, d in re.findall(r'<loc>[^<]*id=(\d+)</loc><lastmod>([\d-]+)</lastmod>', sm) if d >= '2026-03-06']
task('rx-sitemap-recent', C, """
Read https://shop.example.com/sitemap.xml and list the product ids (the id query parameter) whose lastmod
is on or after 2026-03-06.
""", ['http_get'],
    answer={'ids': ('array', 'Integer ids, ascending')},
    expect={'ids': sorted(recent)})

feed = PAGES['https://news.example.com/feed.xml'][2]
items = re.findall(r'<item><title>(.*?)</title><link>(.*?)</link><pubDate>(.*?)</pubDate></item>', feed)
items = sorted(({'title': html.unescape(t), 'published': email.utils.parsedate_to_datetime(d).strftime('%Y-%m-%dT%H:%M:%SZ')}
                for t, _, d in items), key=lambda x: x['published'], reverse=True)
task('rx-rss-items', C, """
Parse the RSS feed at https://news.example.com/feed.xml: for each item, its title (HTML entities decoded)
and its pubDate as an ISO 8601 UTC string like 2026-03-14T15:30:00Z.
""", ['http_get'],
    answer={'items': ('array', 'List of {"title": str, "published": str}, newest first')},
    expect={'items': items})

task('rx-robots', C, """
Read https://shop.example.com/robots.txt and list the paths disallowed for all user agents
("User-agent: *"), plus the sitemap URL.
""", ['http_get'],
    answer={'disallowed': ('array', 'Paths in file order'), 'sitemap': 'Sitemap URL'},
    expect={'disallowed': ['/admin', '/checkout'], 'sitemap': 'https://shop.example.com/sitemap.xml'})

redacted = re.sub(PHONE, '[phone]', re.sub(r'[\w.+-]+@[\w-]+(?:\.[\w-]+)+', '[email]', contract))


def check_redacted(t: Transcript):
    files = [c for c in t.to('write_file') if c.get('path') == '/out/contract_redacted.txt']
    if not files:
        return '/out/contract_redacted.txt was not written'
    if files[-1]['content'] != redacted:
        return 'redacted content differs'
    return None


task('rx-redact', C, """
Write /out/contract_redacted.txt: the text of /docs/contract.txt with every email address replaced by
"[email]" and every phone number replaced by "[phone]". A phone number starts with "+" followed by
digits, spaces, parentheses and hyphens, and ends with a digit. Leave everything else unchanged.
""", ['read_file', 'write_file'], check=check_redacted)

params = Counter()
for line in ACCESS_LOG:
    path = line.split('"')[1].split()[1]
    for k, _ in urllib.parse.parse_qsl(urllib.parse.urlsplit(path).query):
        params[k] += 1
task('rx-query-params', C, """
Which query-string parameter names appear in the request paths of the 2026-03-14 access log, and how
many requests use each?
""", ['get_access_log'],
    answer={'params': ('object', 'Parameter name -> number of requests using it')},
    expect={'params': dict(params)})

about = PAGES['https://shop.example.com/about'][2]
contacts = sorted(set(re.findall(r'[\w.+-]+@[\w-]+(?:\.[\w-]+)+', home + about)))
task('rx-site-contacts', C, """
Collect every email address that appears on https://shop.example.com/ and https://shop.example.com/about
(in text or in mailto: links).
""", ['http_get'],
    answer={'emails': ('array', 'Distinct addresses sorted ascending')},
    expect={'emails': contacts})

task('rx-meta-descriptions', C, """
For product pages https://shop.example.com/products?id=1 through id=12, extract the content of the
<meta name="description"> tag.
""", ['http_get'],
    answer={'descriptions': ('object', 'Page id as a string like "3" -> description text')},
    expect={'descriptions': {str(i): f'Buy the {PRODUCTS[i - 1]["name"]} today' for i in range(1, 13)}})

ratings = {}
for i in range(1, 13):
    rs = [int(x) for x in re.findall(r'data-rating="(\d)"', PAGES[f'https://shop.example.com/products?id={i}'][2])]
    ratings[str(i)] = r2(sum(rs) / len(rs))
task('rx-review-ratings', C, """
Each product page https://shop.example.com/products?id=N (N = 1..12) lists reviews as
<li data-rating="R">. Compute the average rating per page.
""", ['http_get'],
    answer={'ratings': ('object', 'Page id as a string -> average rating rounded to 2 decimals')},
    expect={'ratings': ratings}, tol=0.006)
