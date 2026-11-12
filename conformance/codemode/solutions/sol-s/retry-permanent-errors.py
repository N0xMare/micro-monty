found = {}
not_found = []
for customer_id in [4, 77, 12, 300, 25]:
    try:
        customer = await get_customer({"customer_id": customer_id})
    except LookupError:
        not_found.append(customer_id)
    else:
        found[str(customer_id)] = customer["name"]
await submit_answer({"found": found, "not_found": not_found})
