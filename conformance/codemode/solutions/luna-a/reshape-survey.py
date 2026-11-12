import csv
import io

rows = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/survey.csv"}, newline=""))))
