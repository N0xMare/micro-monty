const log = await tools.get_access_log({ date: "2026-03-14" });
const pat = /^(\S+) \S+ \S+ \[([^\]]*)\] "([^"]*)" (\d{3}) (\S+)/;
const counts = new Map();
let unparsed = 0;
for (const line of log.split("\n")) {
  if (!line.trim()) continue;
  const m = pat.exec(line);
  if (!m) {
    unparsed += 1;
    continue;
  }
  counts.set(m[1], (counts.get(m[1]) ?? 0) + 1);
}
const ranked = [...counts].sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
const top = ranked.slice(0, 3).map(([ip, n]) => ({ ip, requests: n }));
console.log(top, "unparsed:", unparsed);
console.log(await tools.submit_answer({ top }));
