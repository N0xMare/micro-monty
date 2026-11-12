"""Section sizes of an import image (crates/mm-engine/src/image.rs format).

    python3 -I bench/evidence/import_image/parse.py IMAGE...
"""
import struct
import sys

VSIZE = {0: 8, 1: 4, 2: 5, 3: 4, 4: 4, 5: 4}


class R:
    def __init__(self, b):
        self.b, self.p = b, 0

    def u8(self):
        self.p += 1
        return self.b[self.p - 1]

    def u32(self):
        self.p += 4
        return struct.unpack_from("<I", self.b, self.p - 4)[0]

    def u64(self):
        self.p += 8
        return struct.unpack_from("<Q", self.b, self.p - 8)[0]

    def bytes(self):
        n = self.u32()
        self.p += n
        return self.b[self.p - n:self.p]

    def value(self):
        t = self.u8()
        self.p += VSIZE[t]

    def values(self):
        for _ in range(self.u32()):
            self.value()


def parse(b):
    r = R(b)
    sec = {}
    mark = [0]

    def done(name):
        sec[name] = sec.get(name, 0) + r.p - mark[0]
        mark[0] = r.p

    r.p = 20
    units = []
    for _ in range(r.u32()):
        units.append(r.bytes().decode())
        r.u32()
        r.u32()
    deps = [r.bytes().decode() for _ in range(r.u32())]
    for _ in range(r.u32()):
        k = r.u8()
        r.u32() if k in (0, 4) else r.bytes()
    r.value()
    r.value()
    done("header, units, deps, named objects, root")
    nstr = r.u32()
    text = 0
    for _ in range(nstr):
        r.u8()
        text += len(r.bytes())
    done("strings")
    nblk = r.u32()
    for _ in range(nblk):
        r.bytes()
    done("byte blocks")
    nshape = r.u32()
    for _ in range(nshape):
        r.values()
    done("shapes")
    groups = []
    for _ in range(r.u32()):
        r.u32()
        n = r.u32()
        groups.append(n)
        for _ in range(n):
            r.bytes()
    done("prelude unit code")
    ncodes = r.u32()
    for _ in range(ncodes + sum(groups)):
        r.values()
        r.values()
    done("code constants and names")
    nobj = r.u32()
    hdr = []
    for _ in range(nobj):
        r.u32()
        t = struct.unpack_from("<H", b, r.p)[0]
        r.p += 3
        hdr.append((t, r.u32(), r.u32()))
    done("object headers")
    for _, nv, nr in hdr:
        for _ in range(nv):
            r.value()
        r.p += 8 * nr
    done("object bodies")
    for _ in range(2):
        for _ in range(r.u32()):
            r.bytes()
            r.value()
    for _ in range(r.u32()):
        r.value()
        r.u32()
    done("bindings")
    assert r.p == len(b), (r.p, len(b))
    return dict(size=len(b), units=units, deps=deps, strings=nstr, text=text, blocks=nblk,
                shapes=nshape, groups=len(groups), codes=ncodes + sum(groups), objects=nobj,
                sections=sec)


if __name__ == "__main__":
    for path in sys.argv[1:]:
        print(path, parse(open(path, "rb").read()))
