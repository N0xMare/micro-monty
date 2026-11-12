import re
r = re.search(r'(?P<y>\d{4})-(?P<m>\d\d)', 'on 2026-03').group('y')
print(r)
