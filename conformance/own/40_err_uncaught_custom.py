def validate(age):
    if age < 0:
        raise ValueError("age must be non-negative")
    return age

ages = [3, 10, -1]
for a in ages:
    print(validate(a))
