# str: build a string from pieces, split it and replace in it.
parts = []
for i in range(100000):
    parts.append('item' + str(i))
s = ','.join(parts)
words = s.split(',')
replaced = s.replace('item', 'it')
print(len(s), len(words), len(replaced), replaced.count('it9'))
