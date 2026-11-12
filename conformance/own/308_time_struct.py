# time: struct_time and formatting at fixed instants (UTC).
import time

t = time.gmtime(1700000000)
print(t, t.tm_year, t.tm_yday, t.tm_zone, t.tm_gmtoff, tuple(t), len(t), t[0:3])
print(time.strftime('%Y-%m-%d %H:%M:%S %a %b %j %U %w %Z %%', t), time.asctime(t), time.ctime(0) if time.timezone == 0 else 'tz')
print(time.mktime(time.localtime(1700000000)) == 1700000000, time.gmtime(0)[:6], time.gmtime(-86400)[:3])
s = time.strptime('2024-02-29 12:34', '%Y-%m-%d %H:%M')
print(s, time.strftime('%A', s))
print(time.struct_time((2024, 1, 2, 3, 4, 5, 6, 7, 0)).tm_wday, type(time.time()).__name__, type(time.time_ns()).__name__, time.monotonic() <= time.monotonic())
a = time.perf_counter()
time.sleep(0)
print(time.perf_counter() >= a, time.get_clock_info('monotonic').monotonic)
for bad in [lambda: time.gmtime('x'), lambda: time.strftime('%Y', (2024,)), lambda: time.sleep(-1), lambda: time.strptime('x', '%Y')]:
    try:
        bad()
    except (TypeError, ValueError) as e:
        print(type(e).__name__, e)
