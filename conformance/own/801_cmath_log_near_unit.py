import cmath

# Squaring the small component must not round away the real part of log.
small = 2.0 ** -30
for z in (complex(1.0, small), complex(small, -1.0)):
    print(cmath.log(z).real.hex())
