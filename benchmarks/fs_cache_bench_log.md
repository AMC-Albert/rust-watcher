# Filesystem Cache Benchmark Log

This log tracks performance runs of the fs_cache_bench utility, including configuration, directory, and timing results.

| Date       | Directory                 | Node Count | Walk Time (s) | Insert Time (s) | Total Time (s) | Notes               |
| ---------- | ------------------------- | ---------- | ------------- | --------------- | -------------- | ------------------- |
| 2025-06-21 | C:\Users\Albert\_\blender | 2269       | 5.31          | N/A             | 5.31           | Pre-batch, serial   |
| 2025-06-21 | C:\Users\Albert\_\blender | 2269       | 0.052         | 0.084           | 0.136          | Batch insert        |
| 2025-06-21 | C:\Users\Albert\_         | 398752     | 34.07         | 36.36           | 70.43          | Batch insert, large |

*Add new entries below as you run new benchmarks. Update Node Count, Walk Time, Insert Time, and Notes as needed.*
