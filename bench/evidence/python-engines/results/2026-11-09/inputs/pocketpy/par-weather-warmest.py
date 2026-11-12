pass
cities = ['London', 'Paris', 'Berlin', 'Madrid', 'Rome', 'Oslo', 'Tokyo', 'Sydney']
results = [*(get_weather({'city': c}) for c in cities)]
warmest = max(results, key=lambda r: r['temp_c'])['city']
coldest = min(results, key=lambda r: r['temp_c'])['city']
mean_temp = round(sum((r['temp_c'] for r in results)) / len(results), 1)
print(warmest, coldest, mean_temp)
submit_answer({'warmest': warmest, 'coldest': coldest, 'mean_temp_c': mean_temp})
