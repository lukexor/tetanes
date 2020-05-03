const isPaused = (animationId) => {
  return animationId === null;
};

export const setup = (state) => {
  // Set up event handler for ROM input
  document.getElementById("load-rom").addEventListener('change', function(e) {
    const reader = new FileReader();
    const files = this.files;
    if (reader && files.length) {
      reader.readAsArrayBuffer(files[0]);
      reader.onload = () => {
        state.nes.power_cycle();
        const data = new Uint8Array(reader.result);
        state.nes.load_rom(data);
        state.animationId = requestAnimationFrame(state.emulationLoop);
      };
    }
  }, false);

  document.onkeydown = function(e) {
    switch (e.key) {
      case "Escape":
        if (isPaused(state.animationId)) {
          state.emulationLoop();
        } else {
          cancelAnimationFrame(state.animationId);
          state.animationId = null;
        }
        return false;
      case "Enter":
        state.nes.start(true);
        return false;
      case "Shift":
        state.nes.select(true);
        return false;
      case "z":
        state.nes.a(true);
        return false;
      case "x":
        state.nes.b(true);
        return false;
      case "ArrowUp":
        state.nes.up(true);
        return false;
      case "ArrowDown":
        state.nes.down(true);
        return false;
      case "ArrowLeft":
        state.nes.left(true);
        return false;
      case "ArrowRight":
        state.nes.right(true);
        return false;
      default:
    }
  };

  document.onkeyup = function(e) {
    switch (e.key) {
      case "Enter":
        state.nes.start(false);
        return false;
      case "Shift":
        state.nes.select(false);
        return false;
      case "z":
        state.nes.a(false);
        return false;
      case "x":
        state.nes.b(false);
        return false;
      case "ArrowUp":
        state.nes.up(false);
        return false;
      case "ArrowDown":
        state.nes.down(false);
        return false;
      case "ArrowLeft":
        state.nes.left(false);
        return false;
      case "ArrowRight":
        state.nes.right(false);
        return false;
      default:
    }
  };
};
