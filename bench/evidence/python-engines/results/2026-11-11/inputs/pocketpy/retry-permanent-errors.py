pass
ids = [4, 77, 12, 300, 25]

def transient(e):
    return isinstance(e, TimeoutError) or (isinstance(e, RuntimeError) and '503' in str(e))

def fetch(cid):
    for attempt in range(4):
        try:
            return get_customer({'customer_id': cid})
        except LookupError:
            raise
        except Exception as e:
            if not transient(e) or attempt == 3:
                raise
            None
results = [*(fetch(i) for i in ids)]
found, not_found = ({}, [])
for i, r in zip(ids, results):
    if isinstance(r, LookupError):
        not_found.append(i)
    elif isinstance(r, BaseException):
        raise r
    else:
        found[str(i)] = r['name']
print(found, not_found)
print(submit_answer({'found': found, 'not_found': not_found}))
