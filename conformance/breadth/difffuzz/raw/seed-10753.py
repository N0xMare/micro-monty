# difffuzz divergence, seed 10753
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: caught ValueError could not convert string to float: '****-42*****aaa\u1c38' ("could not convert string to float: '****-42*****aaa\\u1c38'",)
#   mm:      caught ValueError could not convert string to float: '****-42*****aaa᰸' ("could not convert string to float: '****-42*****aaa᰸'",)
# signature: CPython: caught ValueError could not convert string to float: '****-#*****aaa\u#c#' ("could not convert strin | mm: caught ValueError could not convert string to float: '****-#*****aaa᰸' ("could not convert string to
import math
try:
    v1 = ('-42'[:].center(12, chr(42)) + ('a' * (255 % 4)).removesuffix(('caf\u00e9' + ''))).rjust(3, chr(42))
    v1 += chr(7224)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print([][(abs(round(float(v1))).bit_length() - +sorted({255, -3, 7, -1})[int(3.0)])])
    except Exception as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
