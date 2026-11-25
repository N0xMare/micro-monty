import traceback


for name in ('decdoe', 'exetnd', 'apend'):
    try:
        getattr(bytearray(), name)
    except AttributeError as exc:
        print(traceback.format_exception(exc)[-1], end='')
