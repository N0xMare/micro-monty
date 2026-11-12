pass
issues, page = ([], 1)
while True:
    res = search_issues({'query': 'status:in_progress', 'page': page})
    issues.extend(res['issues'])
    if not res.get('has_more'):
        break
    page += 1
details = [*(get_issue({'key': i['key']}) for i in issues)]
closed = []
for d in sorted(details, key=lambda d: d['key']):
    if d.get('status') != 'in_progress':
        continue
    comments = d.get('comments') or []
    if comments and comments[-1]['body'].startswith('Fixed in #'):
        update_issue({'key': d['key'], 'fields': {'status': 'closed'}})
        add_comment({'key': d['key'], 'body': 'Closing: fix merged'})
        closed.append(d['key'])
print('checked:', len(details), 'closed:', closed)
if 'submit_answer' in globals():
    submit_answer({'closed': closed})
