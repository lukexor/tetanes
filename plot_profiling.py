import matplotlib.pyplot as plt
import numpy as np
import sys

filename = sys.argv[1]
x_frames, y_time = np.loadtxt(filename, unpack=True)

fig, timing = plt.subplots()

timing.plot(x_frames, y_time, marker='.')
timing.set_title("Loop Frame Budget")
timing.set_xlabel("Frame")
timing.set_ylabel("Time [ms]")

plt.savefig(f"{filename}.png", dpi=300)
