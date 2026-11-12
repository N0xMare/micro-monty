import re
x = re.compile('x|' + '(a)' * 2000).split('x' * 40000)
