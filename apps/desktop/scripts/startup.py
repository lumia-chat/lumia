#!/usr/bin/env python3
"""Times the shell's launch against architecture 17's cold-start row: from starting the
process to its window's first frame on the GPU, over a data directory that already holds a
signed-in session (so the launch draws the last-viewed channel from what the core kept and
from SQLite, before the instance answers).

    scripts/startup.py --data DIR [--bin target/perf/lumia] [--runs 10]

Each run opens the real window for a moment and quits at its first frame. The file cache is
warm after the first run, which is reported apart."""
import argparse, os, statistics, subprocess, sys, time

p = argparse.ArgumentParser()
p.add_argument("--data", required=True)
p.add_argument("--bin", default=os.path.join(os.path.dirname(__file__), "../target/perf/lumia"))
p.add_argument("--runs", type=int, default=10)
a = p.parse_args()

env = dict(os.environ, LUMIA_DATA_DIR=a.data, LUMIA_FIRST_FRAME="quit")
times = []
for _ in range(a.runs + 1):
    start = time.perf_counter()
    proc = subprocess.Popen([a.bin], env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    line = proc.stdout.readline()
    took = (time.perf_counter() - start) * 1000
    proc.wait(timeout=10)
    if line.strip() != "lumia-first-frame":
        sys.exit(f"no first frame: {line!r}")
    times.append(took)
first, rest = times[0], sorted(times[1:])
print(f"first run {first:.0f} ms; then median {statistics.median(rest):.0f} ms, "
      f"min {rest[0]:.0f}, max {rest[-1]:.0f} over {len(rest)} runs")
