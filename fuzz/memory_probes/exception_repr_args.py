e = ValueError(*(['\x00' * 1000] * (M // 3000)))
x = repr(e)
