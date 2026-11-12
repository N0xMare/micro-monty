const services = await tools.list_services({});
const logs = await Promise.all(services.map((service) => tools.fetch_logs({ service, date: "2026-03-14" })));
const levelRe = /^\S+\s+(DEBUG|INFO|WARN|ERROR)\s/;
const counts = {};
services.forEach((svc, i) => {
  let n = 0;
  for (const line of logs[i].split("\n")) {
    const m = levelRe.exec(line);
    if (m && m[1] === "ERROR") n += 1;
  }
  counts[svc] = n;
});
console.log(counts);
console.log(await tools.submit_answer({ counts }));
