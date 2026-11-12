rows = query_database({'sql': "\nSELECT c.id AS id, c.name AS name, SUM(o.total) AS total\nFROM orders o\nJOIN customers c ON c.id = o.customer_id\nWHERE o.status = 'completed'\nGROUP BY c.id, c.name\nORDER BY total DESC\nLIMIT 6\n"})
print(rows)
top = [{'name': r['name'], 'total': round(r['total'], 2)} for r in rows[:5]]
print(submit_answer({'top': top}))
