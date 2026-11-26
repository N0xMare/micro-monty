import traceback


class Names(list):
    pass


for trigger in (lambda: Flase, lambda: Nnoe, lambda: Ture, lambda: Names().apped(1)):
    try:
        trigger()
    except (AttributeError, NameError) as exc:
        print(traceback.format_exception(exc)[-1], end='')
    else:
        raise AssertionError('missing exception')
