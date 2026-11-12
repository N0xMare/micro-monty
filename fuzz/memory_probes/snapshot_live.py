# Many small objects, about 0.4 x the limit, still live when the session
# is snapshotted and restored: the image is about as large as the arena.
keep = ['%08d' % i * 8 for i in range(M // 256)]
