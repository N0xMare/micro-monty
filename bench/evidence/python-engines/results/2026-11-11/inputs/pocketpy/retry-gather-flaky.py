pass
hosts = ['web-01', 'web-02', 'web-03', 'web-04']

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and '503' in str(e))

def fetch(host):
    for attempt in range(4):
        try:
            return get_metrics({'host': host, 'metric': 'cpu'})
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            print(f'{host}: attempt {attempt + 1} failed ({type(e).__name__}: {e}); retrying')
            None
series = [*(fetch(h) for h in hosts)]
max_cpu = {}
for h, s in zip(hosts, series):
    vals = [v for _, v in s['points'] if v is not None]
    max_cpu[h] = max(vals) if vals else None
print(max_cpu)
print(submit_answer({'max_cpu': max_cpu}))
