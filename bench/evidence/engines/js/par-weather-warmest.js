const cities = ["London", "Paris", "Berlin", "Madrid", "Rome", "Oslo", "Tokyo", "Sydney"];
const results = await Promise.all(cities.map((city) => tools.get_weather({ city })));

let warm = results[0];
let cold = results[0];
let sum = 0;
for (const r of results) {
  if (r.temp_c > warm.temp_c) warm = r;
  if (r.temp_c < cold.temp_c) cold = r;
  sum += r.temp_c;
}
const meanTemp = Number((sum / results.length).toFixed(1));
console.log(warm.city, cold.city, meanTemp);
await tools.submit_answer({ warmest: warm.city, coldest: cold.city, mean_temp_c: meanTemp });
