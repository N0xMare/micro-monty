const MAX_ATTEMPTS = 5;
let quote = null;
let attempts = 0;
for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
  attempts = attempt;
  try {
    quote = await tools.get_quote({ symbol: "WAYN" });
    break;
  } catch (e) {
    if (!e.message.includes("429")) throw e;
    console.log(`attempt ${attempt}: ${e.message}`);
    if (attempt === MAX_ATTEMPTS) throw new Error(`WAYN quote still rate limited after ${MAX_ATTEMPTS} attempts`);
  }
}
console.log(quote, "attempts:", attempts);
console.log(await tools.submit_answer({ price: quote.price, attempts }));
