import re, json, collections
r1 = re; del re
#---
import re
r1 is re, re.compile("a+").sub("b", "caa")
