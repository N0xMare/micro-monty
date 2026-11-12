// fmt: format 60k records with template literals (padding, hex, width), the
// equivalent of Python's `%` formatting and `Array.prototype.join`, then join
// them into one report.
function count(s, sub) {
  return s.split(sub).length - 1;
}

const lines = [];
for (let i = 0; i < 60000; i++) {
  const name = `user${String(i).padStart(6, '0')}`;
  const head = `${name}|${String(i % 97).padStart(4)}|${((i * 31) % 4096).toString(16)}`;
  const tail = `${name.slice(-3)}:${i % 13}:${(i % 2 ? 'ok' : 'no').padEnd(5)}`;
  lines.push([head, tail, String(i * 7)].join('|'));
}
const report = lines.join('\n');
console.log(report.length, count(report, 'ok'), count(report, '|  '), lines[12345]);
