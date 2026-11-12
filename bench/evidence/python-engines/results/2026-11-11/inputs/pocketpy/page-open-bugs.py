keys = set()
page = 1
while True:
    resp = search_issues({'query': 'label:bug', 'page': page})
    for i in resp['issues']:
        if 'bug' in i['labels'] and i['status'] != 'closed':
            keys.add(i['key'])
    if not resp.get('has_more'):
        break
    page += 1
result = sorted(keys)
print(len(result), result)
submit_answer({'keys': result})
