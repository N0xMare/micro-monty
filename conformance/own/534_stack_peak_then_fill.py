# A deep recursion's peak, then memory filled: a session restored after the
# recursion returns must have the memory the session that went on has
# (restore-vs-snapshot; the stack's capacity is charged, step 4).
import gc
def deep(n):
    v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14, v15, v16, v17, v18, v19, v20, v21, v22, v23, v24, v25, v26, v27, v28, v29, v30, v31, v32, v33, v34, v35, v36, v37, v38, v39, v40, v41, v42, v43, v44, v45, v46, v47, v48, v49, v50, v51, v52, v53, v54, v55, v56, v57, v58, v59, v60, v61, v62, v63, v64, v65, v66, v67, v68, v69, v70, v71, v72, v73, v74, v75, v76, v77, v78, v79, v80, v81, v82, v83, v84, v85, v86, v87, v88, v89, v90, v91, v92, v93, v94, v95, v96, v97, v98, v99, v100, v101, v102, v103, v104, v105, v106, v107, v108, v109, v110, v111, v112, v113, v114, v115, v116, v117, v118, v119 = (n,) * 120
    if n == 0:
        return 0
    return 1 + deep(n - 1) + (v0 - v119)


depth = deep(990)
junk = 0
for i in range(20000):
    junk += i


# Work after the peak that costs fuel but holds little memory: most parks
# fall here, with a small heap, and few in the fill below, whose images
# are near the limit (with the fill nearly all of the fuel, the release
# gate's 1:1000 run took ~1,000 parks of 100-250 MB images and passed its
# 1,800 s limit).
zeros = 0
block = bytes(1 << 20)
for _ in range(80):
    zeros += block.count(0)
block = None


# Fill up to 320 MiB, or the 256 MiB memory limit if that is lower. The
# counts depend on the memory the session has left, so they stay in
# globals for the final-state digest and are not printed. A park this
# near the limit is refused on the target that charges more (`cross`,
# step 4).
CAP = 320 << 20
counts = [0, 0, 0]
keep = []
total = 0
for k, size in enumerate((1 << 20, 1 << 16, 1 << 10)):
    try:
        while total < CAP:
            keep.append(bytearray(size))
            counts[k] += 1
            total += size
    except MemoryError:
        pass
keep = None
# Nothing frees the blocks until a collection runs, and the fill leaves
# almost no room: `print` does not run again after a collection (spec
# §9.2), so under GC stress its few bytes did not fit (534). The
# collection drops them first, as CPython's reference counts do.
gc.collect()
print(depth, junk, zeros, counts[0] > 0)
