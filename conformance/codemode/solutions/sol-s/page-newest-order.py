result = await list_orders({})
await submit_answer({"order_id": result["orders"][0]["id"]})
