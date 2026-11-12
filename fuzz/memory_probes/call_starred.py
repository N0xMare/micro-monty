class C:
    def m(self, *a): return len(a)
x = C().m(*tuple(range(M // 32)))
