"""URL, encoding and hashing chores."""

import base64
import hashlib
import hmac
import json
import re
import urllib.parse
import zlib

from ptc import Transcript, compare, task
from tasks.worlds import ACCESS_LOG, CUSTOMERS, FILES, PAGES, USERS_TZ

C = 'url-encoding-hashing'

payload = json.loads(base64.b64decode(FILES['/inbox/payload.b64']))
task('enc-base64-payload', C, """
/inbox/payload.b64 holds a base64-encoded JSON document. Decode it and report the batch id, the number of
records and the sum of their amounts.
""", ['read_file'],
    answer={'batch': 'Batch id', 'records': ('integer', ''), 'total': ('number', 'Rounded to 2 decimals')},
    expect={'batch': payload['batch'], 'records': len(payload['records']),
            'total': round(sum(r['amount'] for r in payload['records']), 2)}, tol=0.006)

sums = dict(line.split('  ')[::-1] for line in FILES['/release/SHA256SUMS'].splitlines())
bad = sorted(n for n, h in sums.items() if hashlib.sha256(FILES[f'/release/{n}'].encode()).hexdigest() != h)
task('enc-verify-checksums', C, """
Verify the files listed in /release/SHA256SUMS (lines "<sha256 hex>  <file name>", files in /release/).
Which files do not match their checksum?
""", ['read_file'],
    answer={'mismatched': ('array', 'File names, sorted'), 'checked': ('integer', 'Number of files checked')},
    expect={'mismatched': bad, 'checked': len(sums)})

task('enc-gravatar', C, """
For customers 1 to 5, compute the Gravatar hash of their email: the MD5 hex digest of the email address
trimmed and lowercased.
""", ['get_customer'],
    answer={'hashes': ('object', 'Customer id as string -> md5 hex')},
    expect={'hashes': {str(c['id']): hashlib.md5(c['email'].strip().lower().encode()).hexdigest() for c in CUSTOMERS[:5]}})

params = {'q': 'laptop pro 14"', 'category': 'hardware & more', 'sort': 'price', 'page': 2}
url = 'https://shop.example.com/search?' + urllib.parse.urlencode(params)
task('enc-build-url', C, """
Build the search URL https://shop.example.com/search with the query parameters q = 'laptop pro 14"',
category = 'hardware & more', sort = 'price', page = 2, in that order, encoded as an HTML form query
string (spaces as "+").
""", [],
    answer={'url': 'The full URL'},
    expect={'url': url})

searches = []
for line in ACCESS_LOG:
    path = line.split('"')[1].split()[1]
    parts = urllib.parse.urlsplit(path)
    if parts.path == '/search':
        searches.append(urllib.parse.parse_qs(parts.query)['q'][0])
task('enc-decode-searches', C, """
In the 2026-03-14 access log, find every request to /search and decode its q parameter (percent-encoding
and "+" for spaces). Report the distinct decoded search terms with their counts.
""", ['get_access_log'],
    answer={'terms': ('object', 'Decoded term -> count')},
    expect={'terms': {t: searches.count(t) for t in sorted(set(searches))}})

tok = json.loads(PAGES['https://api.example.com/v1/token'][2])['access_token']
claims = json.loads(base64.urlsafe_b64decode(tok.split('.')[1] + '=='))
task('enc-jwt-claims', C, """
Fetch https://api.example.com/v1/token (JSON with an access_token that is a JWT). Decode the token's
payload (the second dot-separated part, base64url without padding) without verifying the signature, and
report the subject, the scopes and the lifetime in seconds (exp - iat).
""", ['http_get'],
    answer={'sub': 'Subject', 'scopes': ('array', 'Scope strings split on spaces, in order'),
            'lifetime_s': ('integer', 'exp - iat')},
    expect={'sub': claims['sub'], 'scopes': claims['scope'].split(), 'lifetime_s': claims['exp'] - claims['iat']})

body = '{"event":"invoice.paid","id":"evt_123","amount":4200}'
secret = 'whsec_test_5f1c'
sig = hmac.new(secret.encode(), body.encode(), hashlib.sha256).hexdigest()
task('enc-webhook-signature', C, f"""
A webhook arrived with this exact body: {body}
Its X-Signature header is "sha256={sig}". The shared secret is "{secret}". Check the signature: it must be
the hex HMAC-SHA256 of the body bytes (UTF-8) keyed with the secret.
""", [],
    answer={'valid': ('boolean', 'Whether the signature matches'), 'expected_hex': 'The HMAC you computed, hex'},
    expect={'valid': True, 'expected_hex': sig})

report = FILES['/release/report.txt']
task('enc-crc32', C, """
Compute the CRC-32 checksum of /release/report.txt (the UTF-8 bytes of its content) as an unsigned integer
and as 8 lowercase hex digits.
""", ['read_file'],
    answer={'crc32': ('integer', ''), 'hex': '8 hex digits'},
    expect={'crc32': zlib.crc32(report.encode()), 'hex': f'{zlib.crc32(report.encode()):08x}'})

users = list(USERS_TZ)
basic = 'Basic ' + base64.b64encode(b'svc-reporter:s3cr3t:pa55').decode()
task('enc-basic-auth', C, """
Build an HTTP Basic authorization header value for username "svc-reporter" and password "s3cr3t:pa55",
and decode this header back into username and password: "Basic YWRtaW46aHVudGVyMg==".
""", [],
    answer={'header': 'The header value, "Basic <base64>"', 'decoded_user': '', 'decoded_password': ''},
    expect={'header': basic, 'decoded_user': 'admin', 'decoded_password': 'hunter2'})

links = re.findall(r'href="([^"]+)"', PAGES['https://shop.example.com/'][2])
comps = []
for h in links:
    u = urllib.parse.urlsplit(urllib.parse.urljoin('https://shop.example.com/', h.replace('&amp;', '&')))
    if u.scheme == 'https':
        comps.append({'host': u.netloc, 'path': u.path, 'query': dict(urllib.parse.parse_qsl(u.query))})
task('enc-url-components', C, """
Fetch https://shop.example.com/ and for every https link on it (resolve relative hrefs against the page
URL, decode &amp; to &), report the host, path and query parameters, in page order (duplicates included).
""", ['http_get'],
    answer={'links': ('array', 'List of {"host": str, "path": str, "query": {name: value}}')},
    expect={'links': comps})


def slug(s):
    s = re.sub(r'[^a-z0-9]+', '-', s.lower()).strip('-')
    return s


titles = ['Rates hold steady', 'New chip & board launch', 'Storm warning issued', 'Markets rally']
task('enc-slugs', C, """
Fetch https://news.example.com/feed.xml and make a URL slug of every item title: decode HTML entities,
lowercase, replace every run of characters other than a-z and 0-9 with a single "-", and strip leading and
trailing "-".
""", ['http_get'],
    answer={'slugs': ('array', 'Slugs in feed order')},
    expect={'slugs': [slug(t) for t in titles]})

etag = hashlib.sha1(FILES['/docs/CHANGELOG.md'].encode()).hexdigest()[:16]
task('enc-etag', C, """
Compute a weak ETag for /docs/CHANGELOG.md: W/"<first 16 hex digits of the SHA-1 of its UTF-8 content>".
Also report the content length in bytes.
""", ['read_file'],
    answer={'etag': 'The ETag', 'bytes': ('integer', '')},
    expect={'etag': f'W/"{etag}"', 'bytes': len(FILES['/docs/CHANGELOG.md'].encode())})

text = 'Grüße aus Köln – 東京 🚀'
task('enc-utf8-sizes', C, f"""
For the string "{text}", report its length in characters, its UTF-8 size in bytes, its UTF-16-LE size in
bytes, and its UTF-8 bytes as lowercase hex.
""", [],
    answer={'chars': ('integer', ''), 'utf8_bytes': ('integer', ''), 'utf16_bytes': ('integer', ''), 'hex': ''},
    expect={'chars': len(text), 'utf8_bytes': len(text.encode()), 'utf16_bytes': len(text.encode('utf-16-le')),
            'hex': text.encode().hex()})

ids = {}
for c in CUSTOMERS[:3]:
    h = hashlib.sha256(f'tenant-42:{c["email"]}'.encode()).digest()
    ids[str(c['id'])] = base64.b32encode(h[:10]).decode().lower()
task('enc-pseudonyms', C, """
Pseudonymize customers 1, 2 and 3: for each, take the SHA-256 digest of the UTF-8 string
"tenant-42:<email>", keep its first 10 bytes, base32-encode them (RFC 4648, no padding is needed for 10
bytes) and lowercase the result.
""", ['get_customer'],
    answer={'ids': ('object', 'Customer id as string -> pseudonym')},
    expect={'ids': ids})


def check_upload(t: Transcript):
    files = [c for c in t.to('write_file') if c.get('path') == '/out/upload.json']
    if not files:
        return '/out/upload.json was not written'
    try:
        got = json.loads(files[-1]['content'])
    except ValueError:
        return 'not JSON'
    raw = FILES['/docs/contract.txt'].encode()
    return compare({'name': 'contract.txt', 'size': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
                    'content_b64': base64.b64encode(raw).decode()}, got, 1e-9)


task('enc-upload-envelope', C, """
Prepare /docs/contract.txt for upload: write /out/upload.json containing a JSON object with "name"
("contract.txt"), "size" (UTF-8 byte length), "sha256" (hex digest of the UTF-8 bytes) and "content_b64"
(standard base64 of the UTF-8 bytes).
""", ['read_file', 'write_file'], check=check_upload)

qs = 'utm_source=mail&utm_medium=email&id=7&ref=home&utm_campaign=spring&lang=en'
clean = 'https://shop.example.com/products?id=7&ref=home&lang=en'
task('enc-strip-tracking', C, f"""
Remove every utm_* parameter from https://shop.example.com/products?{qs} keeping the other parameters in
their original order.
""", [],
    answer={'url': 'The cleaned URL'},
    expect={'url': clean})
