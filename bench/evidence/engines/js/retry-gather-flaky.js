// pi has no timers, so retries go again at once (the Python solution sleeps).
const hosts = ["web-01", "web-02", "web-03", "web-04"];
const transient = (e) => e.type === "TimeoutError" || (e.type === "RuntimeError" && e.message.includes("503"));

async function fetch(host) {
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      return await tools.get_metrics({ host, metric: "cpu" });
    } catch (e) {
      if (!transient(e) || attempt === 3) throw e;
      console.log(`${host}: attempt ${attempt + 1} failed (${e.type}: ${e.message}); retrying`);
    }
  }
}

const series = await Promise.all(hosts.map(fetch));
const maxCpu = {};
hosts.forEach((h, i) => {
  const vals = series[i].points.map(([, v]) => v).filter((v) => v !== null);
  maxCpu[h] = vals.length ? Math.max(...vals) : null;
});
console.log(maxCpu);
console.log(await tools.submit_answer({ max_cpu: maxCpu }));
