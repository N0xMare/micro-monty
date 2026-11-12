# utf-8-sig: BOM written on encode, skipped once on decode, error positions
# after the BOM, codecs.lookup/encode/decode, csv and json over it.
import codecs
for b in [b'\xef\xbb\xbfab\xff', b'ab\xff', b'\xef\xbb\xbf', b'\xef\xbb', b'\xef\xbb\xbf\xef\xbb\xbfx', b'\xef\xbb\xbfh\xc3\xa9llo']:
    try: print(repr(b.decode('utf-8-sig')))
    except UnicodeDecodeError as e: print(e)
print('é'.encode('utf-8-sig'), ''.encode('utf_8_sig'), 'x'.encode('UTF-8-SIG'))
print(b'\xef\xbb\xbfab\xff'.decode('utf-8-sig', 'replace'), b'ab\xff\xfe'.decode('utf-8-sig', 'backslashreplace'))
print(codecs.lookup('utf-8-sig').name, codecs.encode('a', 'utf-8-sig'), codecs.decode(codecs.BOM_UTF8 + b'z', 'utf-8-sig'))
import csv, io, json
raw = '\ufeffname,qty\nx,1\n'.encode('utf-8')
print(list(csv.DictReader(io.StringIO(raw.decode('utf-8-sig')))))
print(json.loads(b'\xef\xbb\xbf{"a": 1}'.decode('utf-8-sig')))
try: 'x'.encode('utf8-sig')
except LookupError as e: print('LookupError', e)
