from datetime import datetime, timedelta
r = (datetime(2026, 3, 15) + timedelta(days=20)).strftime('%Y-%m-%d %a')
print(r)
