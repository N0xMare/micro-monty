# int / int with a subnormal quotient is not correctly rounded in mm
# (one ulp off); CPython rounds the exact quotient once.
print(6 / (int(1e300) << 30))
