import matplotlib.pyplot as plt
import numpy as np

x_frames, y_buffer, y_pitch= np.loadtxt("./stats.dat", unpack=True)

frames = 200

fig, (buffer, pitch) = plt.subplots(2, sharex=True)

buffer.plot(x_frames, y_buffer, marker = '.')
buffer.set_title("Audio buffer size (AB = 4096)")
buffer.set_xlabel("Frame")
buffer.set_ylabel("Ab")
# buffer.ticklabel_format(style='sci', axis='x', scilimits=(0,0))
buffer.set_ylim([0, 4096])
buffer.set_xlim([0, frames])

pitch.plot(x_frames, y_pitch, marker = '.')
# pitch.ticklabel_format(style='sci', axis='x', scilimits=(0,0))
pitch.set_title("Relative Pitch")
pitch.set_xlabel("Frame")
pitch.set_ylabel("Pitch Mod")
# pitch.set_ylim([0.995, 1.005])
pitch.set_xlim([0, frames])

plt.subplots_adjust(hspace=0.5)

plt.show()
