const ids = [4, 77, 12, 300, 25];
const transient = (e) => e.type === "TimeoutError" || (e.type === "RuntimeError" && e.message.includes("503"));

async function fetch(cid) {
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      return await tools.get_customer({ customer_id: cid });
    } catch (e) {
      if (e.type === "LookupError") throw e; // permanent: 404, do not retry
      if (!transient(e) || attempt === 3) throw e;
    }
  }
}

const results = await Promise.allSettled(ids.map(fetch));
const found = {};
const notFound = [];
ids.forEach((id, i) => {
  const r = results[i];
  if (r.status === "rejected") {
    if (r.reason.type === "LookupError") notFound.push(id);
    else throw r.reason;
  } else found[String(id)] = r.value.name;
});
console.log(found, notFound);
console.log(await tools.submit_answer({ found, not_found: notFound }));
