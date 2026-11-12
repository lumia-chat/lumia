#!/usr/bin/env python3
"""Process launch to first readable pixels of the last-viewed channel (architecture 17, cold start).

Runs the perf build headless with --first-pixels, one warmup then RUNS launches, and reports the
time from spawn to Rune's marker: loader, store open, the channel read through the core, GPU
init, first frame and readback. Offscreen, so a display's present is not in it, and the OS file
cache is warm after the warmup.

usage: startup.py [--limit N] [--runs N]
"""
import os
import statistics
import subprocess
import sys
import time

here = os.path.dirname(os.path.abspath(__file__))
binary = os.path.join(here, "..", "target", "perf", "lumia-rune-spike")


def arg(name, default):
    return int(sys.argv[sys.argv.index(name) + 1]) if name in sys.argv else default


def launch(limit):
    env = dict(os.environ, LUMIA_SPIKE_LIMIT=str(limit), LUMIA_SPIKE_TIMING="1")
    start = time.perf_counter()
    proc = subprocess.Popen([binary, "--first-pixels"], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    marker = None
    for line in proc.stdout:
        if line.startswith("rune-first-pixels"):
            marker = (time.perf_counter() - start) * 1000
            break
    proc.stdout.read()
    stderr = proc.stderr.read()
    proc.wait()
    init = next((float(l.split()[2]) for l in stderr.splitlines() if l.startswith("lumia-spike init")), float("nan"))
    return marker, init


limit, runs = arg("--limit", 10000), arg("--runs", 7)
launch(limit)
samples = [launch(limit) for _ in range(runs)]
first = sorted(s[0] for s in samples)
print(f"limit {limit}: first pixels median {statistics.median(first):.0f} ms (min {first[0]:.0f}, max {first[-1]:.0f}); "
      f"of which store open and channel read {statistics.median(s[1] for s in samples):.1f} ms")
