# Medium blocks that outlive collections beside kept small objects, and a
# churn that replaces live medium blocks with slightly longer ones: the
# room a live block keeps through a sweep and the placement among many
# free blocks a few units too short. Under durability every park is
# restored in a new process, so the allocator's choices must follow the
# image alone (restore-vs-snapshot, cross).
from collections import deque

keep = []
win = deque(maxlen=40)
for n in range(64, 6000, 3):
    win.append([None] * n)
    keep.append((n,))
print(len(keep), len(win), len(win[0]), len(win[-1]))

blocks = [[None] * (100 + i % 50) for i in range(2000)]
for rnd in range(8):
    for j in range(rnd % 2, len(blocks), 2):
        blocks[j] = [None] * (len(blocks[j]) + 3)
        if j % 64 == 0:
            keep.append((rnd, j))
print(len(keep), sum(len(b) for b in blocks))

text = ''
parts = []
for i in range(3000):
    text = text + str(i % 10)
    parts.append(text[-3:])
print(len(text), parts[-1], len(set(parts)))
