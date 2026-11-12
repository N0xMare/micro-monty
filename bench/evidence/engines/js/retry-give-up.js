const urls = [
  "https://api.example.com",
  "https://auth.example.com",
  "https://billing.example.com",
  "https://cdn.example.com",
  "https://mail.example.com",
  "https://search.example.com",
  "https://status.example.com",
];
const transient = (e) => e.type === "TimeoutError" || (e.type === "RuntimeError" && e.message.includes("503"));

async function probe(url) {
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      return await tools.check_health({ url });
    } catch (e) {
      if (!transient(e) || attempt === 3) throw e;
    }
  }
}

const results = await Promise.allSettled(urls.map(probe));
const up = [];
const unreachable = [];
const down = [];
urls.forEach((u, i) => {
  const r = results[i];
  if (r.status === "rejected") {
    console.log(`${u}: unreachable (${r.reason.type}: ${r.reason.message})`);
    unreachable.push(u);
  } else if (r.value.up) up.push(u);
  else down.push(u);
});
console.log("up:", up.sort());
console.log("down (responded, not up):", down.sort());
console.log("unreachable:", unreachable.sort());
console.log(await tools.submit_answer({ up: up.sort(), unreachable: unreachable.sort() }));
