# difffuzz divergence, seed 27434
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError could not convert string to float: '\u2ef7'
#   mm:      EXC ValueError could not convert string to float: '⻷'
# signature: CPython: EXC ValueError could not convert string to float: '\u#ef#' | mm: EXC ValueError could not convert string to float: '⻷'
import math
try:
        for i18 in list({str(q): q for q in [i + v for i, v in enumerate([1000, 1, 3, 1000, 3], 0)]}.values()):
            if i18 == 3:
                break
            try:
                print(tuple(sorted((dict(sorted({'k0': 2}.items(), key=lambda kv: kv[1])) | dict(sorted({k20: len(k20) for k20 in ['  pad  ', 'x\ny', '\u00df']}.items(), key=lambda kv: kv[1]))).items()))[int(float(chr(12023)))])
            except ZeroDivisionError as e:
                print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
