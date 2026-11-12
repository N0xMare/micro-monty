# difffuzz divergence, seed 20155
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ('123', -3, 400000000000000021001904102081768099481787432443263661966341646204720983195563278314548550032179145617481777533153551270777009294144172230257916873914682793139354880370630321495132093517915236023747581293988319978032447615587056352029861097112056997831703515528022737135246267788878554746
#   mm:      ('123', -3, 400000000000000021001904102081768099481787432443263661966341646204720983195563278314548550032179145617481777533153551270777009294144172230257916873914682793139354880370630321495132093517915236023747581293988319978032447615587056352029861097112056997831703515528022737135246267788878554746
# signature: CPython: ('#', -#, #, #, '\u#ffc', '#.#')a,b,,cEXC ValueError invalid literal for int() with base #: 'ǿMissis | mm: ('#', -#, #, #, '࿼', '#.#')a,b,,cEXC ValueError invalid literal for int() with base #: 'ǿMississippi
import math
try:
    print((('123'.upper().split()[0], -3, (math.ceil(1e300) << (math.trunc(2.5) % 70)), ) + ((int(1.5) ** ((10 | 255) % 7)), (chr(4092) + '\U0001f600 ok'.partition('s')[1]), '3.5'.zfill(2).rstrip(), )), sep='', end='a,b,,c')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    try:
        print(pow(int((chr(511) + 'mississippi'.capitalize()), 0), 0, 1000) // abs((abs(len([100, 2])).bit_length() ** (int(f'{255:,} and {v1!r} and {1:10.3e} and ') % 7))).bit_length())
    except KeyError as e:
        print("caught", type(e).__name__, e, e.args)
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
