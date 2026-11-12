#!/usr/bin/env python3
"""Process launch to a real window's first frame (architecture 17, cold start), for either spike.

Launches BINARY with LUMIA_SPIKE_QUIT_AFTER_FIRST_FRAME, one warmup then RUNS launches, and
reports the time from spawn to its `lumia-first-frame` line: loader, store open, the channel
read through the core, window and GPU setup, and the first frame. The OS file cache is warm
after the warmup. Rune prints the line when the frame's GPU work completes; GPUI prints it at
the start of the following frame, up to one refresh interval later.

usage: window-startup.py BINARY [--limit N] [--runs N]
"""
import os
import statistics
import subprocess
import sys
import time


def arg(name, default):
    return int(sys.argv[sys.argv.index(name) + 1]) if name in sys.argv else default


def launch(binary, limit):
    env = dict(os.environ, LUMIA_SPIKE_LIMIT=str(limit), LUMIA_SPIKE_TIMING="1", LUMIA_SPIKE_QUIT_AFTER_FIRST_FRAME="1")
    start = time.perf_counter()
    proc = subprocess.Popen([binary], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    marker = None
    for line in proc.stdout:
        if line.startswith("lumia-first-frame"):
            marker = (time.perf_counter() - start) * 1000
            break
    proc.stdout.read()
    stderr = proc.stderr.read()
    proc.wait()
    init = next((float(l.split()[2]) for l in stderr.splitlines() if l.startswith("lumia-spike init")), float("nan"))
    return marker, init


binary, limit, runs = sys.argv[1], arg("--limit", 10000), arg("--runs", 7)
launch(binary, limit)
samples = [launch(binary, limit) for _ in range(runs)]
first = sorted(s[0] for s in samples)
print(f"{os.path.basename(binary)} limit {limit}: first frame median {statistics.median(first):.0f} ms "
      f"(min {first[0]:.0f}, max {first[-1]:.0f}); store open and channel read {statistics.median(s[1] for s in samples):.1f} ms")
