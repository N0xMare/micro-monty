async function waitingRun() {
  const before = 9;
  const answer = await later(33);
  globalThis.probeResult = before + answer;
  return before + answer;
}
waitingRun();
