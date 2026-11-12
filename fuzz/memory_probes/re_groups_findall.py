import re
x = re.compile('x|' + '(a)' * 20000).findall('x' * 40000)
