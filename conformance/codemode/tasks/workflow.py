"""Multistep workflows with conditionals and early exit."""

import datetime as dt
import json
import re

from ptc import Transcript, compare, task
from tasks.worlds import CITIES, CUSTOMERS, DEPLOYS, ENDPOINTS, FILES, ISSUES, ORDERS, PRODUCTS

C = 'multistep-workflow'


def calls(t: Transcript, name):
    return t.to(name)


# --- assign unassigned open P1 issues to the least-loaded on-call member
oncall = ['bjorn', 'emi']
load = {u: sum(1 for i in ISSUES if i['assignee'] == u and i['status'] != 'closed') for u in oncall}
assign = []
for i in sorted((i for i in ISSUES if i['status'] == 'open' and i['priority'] == 'P1' and i['assignee'] is None),
                key=lambda i: i['key']):
    u = min(oncall, key=lambda u: (load[u], u))
    load[u] += 1
    assign.append((i['key'], u))


def check_assign(t: Transcript):
    ups = sorted((c['key'], c['fields'].get('assignee')) for c in calls(t, 'update_issue'))
    if ups != sorted(assign):
        return f'expected assignments {sorted(assign)}, got {ups}'
    coms = sorted((c['key'], c['body']) for c in calls(t, 'add_comment'))
    want = sorted((k, f'Auto-assigned to {u}') for k, u in assign)
    return None if coms == want else f'expected comments {want}, got {coms}'


task('wf-assign-p1', C, """
Find the open P1 issues that have no assignee. Assign each one, in issue-key order, to the on-call team
member with the fewest non-closed issues currently assigned to them (ties: alphabetically first
username); count each new assignment toward that member's load before assigning the next issue. Set the
assignee with update_issue (fields {"assignee": username}) and add the comment "Auto-assigned to
<username>" to each issue.
""", ['list_team', 'search_issues', 'update_issue', 'add_comment'], check=check_assign)

TODAY = dt.date(2026, 3, 15)


def refund_task(tid, oid):
    o = next(o for o in ORDERS if o['id'] == oid)
    ok = o['status'] == 'completed' and (TODAY - dt.date.fromisoformat(o['order_date'])).days <= 30
    cust = next(c for c in CUSTOMERS if c['id'] == o['customer_id'])
    subject = f'Refund {"approved" if ok else "declined"} for order {oid}'

    def check(t):
        mails = calls(t, 'send_email')
        if len(mails) != 1:
            return f'expected one email, got {len(mails)}'
        return compare({'to': cust['email'], 'subject': subject},
                       {'to': mails[0].get('to'), 'subject': mails[0].get('subject')}, 1e-9)

    task(tid, C, f"""
A customer requested a refund for order {oid}. Today is 2026-03-15. The refund is approved only if the
order's status is "completed" and it was placed at most 30 days ago (2026-02-13 or later). Email the
customer at their address on file with the subject "Refund approved for order {oid}" or "Refund declined
for order {oid}" and a short explanatory body.
""", ['get_order', 'get_customer', 'send_email'], check=check)


recent_done = max((o for o in ORDERS if o['status'] == 'completed'), key=lambda o: o['order_date'])
refund_task('wf-refund-approve', recent_done['id'])
refund_task('wf-refund-decline', next(o['id'] for o in ORDERS if o['status'] == 'pending'))

# --- early exit over a fixed product list
ok_stock = [p['id'] for p in PRODUCTS if p['stock'] >= 10]
low = next(p['id'] for p in PRODUCTS if p['stock'] < 10 and p['id'] not in ok_stock[:3])
seq = ok_stock[:3] + [low] + ok_stock[3:5]


def check_stock(t: Transcript):
    asked = [c['product_id'] for c in calls(t, 'get_product')]
    if asked != seq[:4]:
        return f'expected get_product calls {seq[:4]} (stopping at the first low one), got {asked}'
    return compare({'product_id': low}, t.answer() or {}, 1e-9)


task('wf-first-low-stock', C, f"""
Check the stock of these products one at a time, in this order: {', '.join(seq)}. Stop at the first
product with fewer than 10 units in stock (do not look up the remaining ones) and report it. If none is
low, report null.
""", ['get_product'], answer={'product_id': 'The first low-stock product id, or null'}, check=check_stock)

order = ['https://api.example.com', 'https://auth.example.com', 'https://billing.example.com',
         'https://search.example.com']
down = next(u for u in order if not ENDPOINTS[u][0])


def check_gate(t: Transcript):
    probed = [c['url'] for c in calls(t, 'check_health')]
    if probed != order[:order.index(down) + 1]:
        return f'expected probes up to {down}, got {probed}'
    posts = calls(t, 'post_message')
    want = [{'channel': '#ops', 'text': f'Deploy blocked: {down} is down'}]
    return compare(want, posts, 1e-9)


task('wf-health-gate', C, f"""
Before a deploy, probe these endpoints in order: {', '.join(order)}. As soon as one is down, post
"Deploy blocked: <url> is down" to #ops and stop without probing the rest. If all are up, post
"All clear" to #ops.
""", ['check_health', 'post_message'], check=check_gate)

last_billing = [d for d in DEPLOYS if d['service'] == 'billing'][-1]


def check_deploy(t: Transcript):
    inc = calls(t, 'create_incident')
    if last_billing['status'] == 'success':
        return None if not inc and t.answer() == {'status': 'ok'} else 'expected no incident and status ok'
    if len(inc) != 1:
        return f'expected one incident, got {len(inc)}'
    return compare({'title': f'billing: last deploy {last_billing["status"]}', 'severity': 'sev3',
                    'services': ['billing']}, {k: inc[0].get(k) for k in ('title', 'severity', 'services')}, 1e-9)


task('wf-last-deploy', C, """
Look at the most recent deployment of the billing service. If its status is not "success", open an
incident titled "billing: last deploy <status>" with severity sev3 and services ["billing"]. Otherwise
submit {"status": "ok"}.
""", ['list_deployments', 'create_incident'], answer={'status': 'Only when no incident is needed: "ok"'},
    check=check_deploy)

target = CUSTOMERS[22]
mine = sorted((o for o in ORDERS if o['customer_id'] == target['id']), key=lambda o: (o['order_date'], o['id']))
latest = mine[-1]


def check_lookup(t: Transcript):
    mails = calls(t, 'send_email')
    action = 'emailed' if latest['status'] == 'pending' else 'none'
    if (len(mails) == 1) != (action == 'emailed'):
        return f'expected {"one email" if action == "emailed" else "no email"}, got {len(mails)}'
    if mails and mails[0].get('subject') != f'Your order {latest["id"]} is still pending':
        return f'wrong subject {mails[0].get("subject")!r}'
    return compare({'order_id': latest['id'], 'action': action}, t.answer() or {}, 1e-9)


task('wf-customer-latest-order', C, f"""
Find the customer whose email is {target['email']} (there is no search by email; list the customers).
Look at their most recent order (latest order_date, ties by highest id). If it is pending, email them
with the subject "Your order <id> is still pending". Report the order id and whether you emailed.
""", ['list_customers', 'list_orders', 'send_email'],
    answer={'order_id': ('integer', 'The most recent order id'), 'action': '"emailed" or "none"'},
    check=check_lookup)

sec = sorted(i['key'] for i in ISSUES if i['status'] == 'open' and 'security' in i['labels'] and i['priority'] in ('P3', 'P4'))


def check_sec(t: Transcript):
    ups = sorted(c['key'] for c in calls(t, 'update_issue') if c.get('fields') == {'priority': 'P2'})
    if ups != sec or len(calls(t, 'update_issue')) != len(sec):
        return f'expected priority updates on {sec}, got {[c["key"] for c in calls(t, "update_issue")]}'
    return compare({'changed': sec}, t.answer() or {}, 1e-9)


task('wf-security-priority', C, """
Open issues labelled "security" must be at least P2. Raise every open security issue with priority P3
or P4 to P2 (update_issue with fields {"priority": "P2"}), and report which ones you changed.
""", ['search_issues', 'update_issue'], answer={'changed': ('array', 'Changed issue keys, sorted ascending')},
    check=check_sec)


def job_task(tid, plan, status):
    def check(t: Transcript):
        polls = len(calls(t, 'get_job_status'))
        if polls != len(plan):
            return f'expected {len(plan)} polls (stop when finished), got {polls}'
        want = {'state': status, 'result_url': 'https://api.example.com/exports/job-77.csv' if status == 'done' else None,
                'polls': len(plan)}
        return compare(want, t.answer() or {}, 1e-9)

    task(tid, C, """
Start an export of the "orders" dataset, then poll the job's status until it is "done" or "failed"
(poll at most 10 times; wait 2 seconds between polls). Report the final state, the result URL (null if
none) and how many times you polled.
""", ['start_export', 'get_job_status'],
        answer={'state': '"done" or "failed"', 'result_url': 'The result URL or null', 'polls': ('integer', 'Number of status calls')},
        check=check, config={'exports': {'orders': 'job-77'}, 'jobs': {'job-77': plan}})


job_task('wf-poll-export', ['queued', 'running', 'running', 'done'], 'done')
job_task('wf-poll-export-fails', ['queued', 'running', 'failed'], 'failed')

hours = sum(i['estimate_hours'] or 0 for i in ISSUES if i['assignee'] == 'bjorn' and i['status'] != 'closed')


def check_overload(t: Transcript):
    posts = calls(t, 'post_message')
    if hours > 20:
        return compare([{'channel': '#eng', 'text': f'bjorn is overloaded ({hours}h)'}], posts, 1e-9)
    return None if not posts else 'expected no message'


task('wf-overload-alert', C, """
Sum estimate_hours (treat null as 0) over the non-closed issues assigned to bjorn. If the total is above
20, post "bjorn is overloaded (<total>h)" to #eng, e.g. "bjorn is overloaded (27h)". Otherwise post
nothing.
""", ['search_issues', 'post_message'], check=check_overload)

terms = ['tablet', 'monitor', 'hub']
hit_term = next(t for t in terms if any(t in p['name'].lower() for p in PRODUCTS))
cheapest = min((p for p in PRODUCTS if hit_term in p['name'].lower()), key=lambda p: p['price'])


def check_search(t: Transcript):
    urls = [c['url'] for c in calls(t, 'http_get')]
    if len(urls) != terms.index(hit_term) + 1:
        return f'expected {terms.index(hit_term) + 1} searches, got {urls}'
    return compare({'term': hit_term, 'product_id': cheapest['id']}, t.answer() or {}, 1e-9)


task('wf-first-search-hit', C, f"""
Search the shop for these terms, in order, with https://shop.example.com/api/search?q=<term> (JSON
body: {{"query": str, "results": [{{"id": str, "name": str, "price": float}}]}}): {', '.join(terms)}. Stop
at the first term that has any results and report it with the id of its cheapest result.
""", ['http_get'], answer={'term': 'The first term with results', 'product_id': 'Cheapest result id'},
    check=check_search)

cities = ['Oslo', 'Berlin', 'Madrid', 'Rome', 'Sydney']
pick = next(c for c in cities if CITIES[c][0] >= 15 and CITIES[c][1] == 'sunny')


def check_trip(t: Transcript):
    asked = [c['city'] for c in calls(t, 'get_weather')]
    if asked != cities[:cities.index(pick) + 1]:
        return f'expected weather lookups up to {pick}, got {asked}'
    return compare({'city': pick}, t.answer() or {}, 1e-9)


task('wf-first-sunny-city', C, f"""
Check the weather of these cities one by one, in order: {', '.join(cities)}. Pick the first city that is
"sunny" with a temperature of at least 15 °C and stop checking further cities.
""", ['get_weather'], answer={'city': 'The chosen city'}, check=check_trip)

inv = json.loads(FILES['/data/inventory.json'])
totals = {w['warehouse']: sum(b['qty'] for b in w['bins']) for w in inv['warehouses']}


def check_inv(t: Transcript):
    files = [c for c in calls(t, 'write_file') if c.get('path') == '/out/summary.json']
    if not files:
        return '/out/summary.json was not written'
    try:
        got = json.loads(files[-1]['content'])
    except ValueError:
        return 'summary is not JSON'
    return compare(totals, got, 1e-9) or compare({'written': True}, t.answer() or {}, 1e-9)


task('wf-validate-then-write', C, """
Read /data/inventory.json. If any warehouse's "updated" timestamp is before 2026-03-14T00:00:00Z, write
nothing and submit {"written": false}. Otherwise write /out/summary.json containing a JSON object that maps
each warehouse to its total qty over all bins, and submit {"written": true}.
""", ['read_file', 'write_file'], answer={'written': ('boolean', 'Whether the summary was written')}, check=check_inv)

west_gold = sorted({c['email'] for c in CUSTOMERS if c['tier'] == 'gold' and c['region'] == 'West'})


def check_news(t: Transcript):
    mails = calls(t, 'send_email')
    tos = sorted(m.get('to') for m in mails)
    if tos != west_gold:
        return f'expected emails to {west_gold}, got {tos}'
    if any(m.get('subject') != 'March newsletter' for m in mails):
        return 'wrong subject'
    for m in mails:
        first = next(c['name'].split()[0] for c in CUSTOMERS if c['email'] == m['to'])
        if not str(m.get('body', '')).startswith(f'Hello {first},'):
            return f'body for {m["to"]} should start with "Hello {first},"'
    return None


task('wf-newsletter', C, """
Send the "March newsletter" email (subject exactly "March newsletter") to every gold-tier customer in the
West region, once per address. The body must start with "Hello <first name>," followed by a short
announcement of your choice.
""", ['list_customers', 'send_email'], check=check_news)

closable = []
for i in ISSUES:
    if i['status'] == 'in_progress' and i['comments'] and re.match(r'Fixed in #\d+', i['comments'][-1]['body']):
        closable.append(i['key'])


def check_close(t: Transcript):
    ups = sorted(c['key'] for c in calls(t, 'update_issue') if c.get('fields') == {'status': 'closed'})
    if ups != sorted(closable) or len(calls(t, 'update_issue')) != len(closable):
        return f'expected to close {closable}, got {[c["key"] for c in calls(t, "update_issue")]}'
    coms = sorted(c['key'] for c in calls(t, 'add_comment') if c.get('body') == 'Closing: fix merged')
    return None if coms == sorted(closable) else f'expected comments on {closable}, got {coms}'


task('wf-close-fixed', C, """
For every in_progress issue whose most recent comment starts with "Fixed in #", set its status to closed
(update_issue with fields {"status": "closed"}) and add the comment "Closing: fix merged".
""", ['search_issues', 'get_issue', 'update_issue', 'add_comment'], check=check_close)


def check_fallback(t: Transcript):
    urls = [c['url'] for c in calls(t, 'http_get')]
    if urls[:1] != ['https://shop.example.com/team']:
        return f'expected to try /team first, got {urls}'
    return compare({'phone': '+1-555-0199', 'source': 'https://shop.example.com/about'}, t.answer() or {}, 1e-9)


task('wf-fallback-page', C, """
Get the shop's office phone number. Try https://shop.example.com/team first; if that page is not found
(status 404), use https://shop.example.com/about instead. Report the phone number (as written, it starts
with "+") and which URL it came from.
""", ['http_get'], answer={'phone': 'Phone number', 'source': 'URL of the page it came from'}, check=check_fallback)
