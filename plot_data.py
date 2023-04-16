import matplotlib.pyplot as plt
import numpy as np

data = np.loadtxt("./data.dat", unpack=True)

val = 0
plt.plot(data)
plt.show()
