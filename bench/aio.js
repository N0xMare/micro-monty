// aio: an await loop over an async function, Promise.all over batches of
// calls, and tasks started at once that yield to the job queue.
async function step(i) {
  return (i * 3) % 7;
}

async function awaitLoop(n) {
  let total = 0;
  for (let i = 0; i < n; i++) {
    total += await step(i);
  }
  return total;
}

async function worker(k, n) {
  let acc = 0;
  for (let i = 0; i < n; i++) {
    await null;
    acc += (i ^ k) & 15;
  }
  return acc;
}

async function main() {
  const looped = await awaitLoop(150000);
  let gathered = 0;
  for (let r = 0; r < 300; r++) {
    const calls = [];
    for (let j = 0; j < 50; j++) {
      calls.push(step(r + j));
    }
    const values = await Promise.all(calls);
    gathered += values.reduce((a, b) => a + b, 0);
  }
  const tasks = [];
  for (let k = 0; k < 20; k++) {
    tasks.push(worker(k, 1500));
  }
  let done = 0;
  for (const task of tasks) {
    done += await task;
  }
  console.log(looped, gathered, done);
}

main();
