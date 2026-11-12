import csv
import io
import statistics
from collections import defaultdict
text = await read_file({"path": "/data/employees.csv"})
groups = defaultdict(list)
for row in csv.DictReader(io.StringIO(text)):
    groups[row["department"]].append(float(row["salary"].replace(",", "").replace("$", "")))
await submit_answer({"departments": {department: {"median": statistics.median(salaries), "mean": round(statistics.mean(salaries), 2)} for department, salaries in groups.items()}})
