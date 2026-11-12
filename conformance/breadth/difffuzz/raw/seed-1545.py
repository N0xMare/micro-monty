# difffuzz divergence, seed 1545
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: '\u13fft\u13ffa\u13ffb\u13ff\t\u13ffh\u13ffe\u13ffr\u13ffe\u13ff-\u13ff4\u13ff2\u13ff'
#   mm:      EXC ValueError invalid literal for int() with base 10: '᏿t᏿a᏿b᏿\t᏿h᏿e᏿r᏿e᏿-᏿4᏿2᏿'
# signature: CPython: EXC ValueError invalid literal for int() with base #: '\u#fft\u#ffa\u#ffb\u#ff\t\u#ffh\u#ffe\u#ffr\u | mm: EXC ValueError invalid literal for int() with base #: '᏿t᏿a᏿b᏿\t᏿h᏿e᏿r᏿e᏿-᏿#᏿#᏿'
import math
try:
    match int(('tab\there' + '-42').ljust(3, chr(45)).replace(('a,b,,c' * (0 % 4))[-2:6], chr(5119))):
        case {}:
            print("any mapping")
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
