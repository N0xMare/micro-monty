import csv
import io
from collections import defaultdict
from statistics import mean, median

text = await read_file({"path": "/data/employees.csv"})
departments = defaultdict(list)
for row in csv.DictReader(io.StringIO(text)):
    departments[row["department"]].append(float(row["salary"].replace(",", "").replace("$", "")))
await submit_answer({"departments": {name: {"median": median(values), "mean": round(mean(values), 2)} for name, values in departments.items()}})
