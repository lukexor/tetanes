const keybinds = [
  'Escape',
  'Enter',
  'Shift',
  'a',
  's',
  'z',
  'x',
  'ArrowUp',
  'ArrowDown',
  'ArrowLeft',
  'ArrowRight',
];

export const eventPump = [];

export const setup = (state) => {
  eventPump.splice(0);

  document.getElementById('load-rom').addEventListener('click', function(e) {
    state.nes.pause(true);
  });


  // Set up event handler for ROM input
  document.getElementById('load-rom').addEventListener('change', function(e) {
    const reader = new FileReader();
    const files = this.files;
    if (reader && files.length) {
      reader.readAsArrayBuffer(files[0]);
      reader.onload = () => {
        const data = new Uint8Array(reader.result);
        state.nes.load_rom(data);
        if (!state.animationId) {
          state.animationId = requestAnimationFrame(state.emulationLoop);
        }
      };
    }
  }, false);

  document.onkeydown = function(e) {
    if (keybinds.includes(e.key) && !e.repeat) {
      eventPump.push({ key: e.key, pressed: true, repeat: e.repeat });
      e.preventDefault();
      return false;
    }
  };

  document.onkeyup = function(e) {
    if (keybinds.includes(e.key) && !e.repeat) {
      eventPump.push({ key: e.key, pressed: false, repeat: e.repeat });
      e.preventDefault();
      return false;
    }
  };
};

export const handleEvents = (state) => {
  const events = eventPump.splice(0);
  events.forEach(e => {
      state.nes.handle_event(e.key, e.pressed, e.repeat);
  });
};

export const fps = new class {
  constructor() {
    this.fps = document.getElementById('fps');
    this.frames = [];
    this.lastFrameTimeStamp = performance.now();
  }

  render() {
    const now = performance.now();
    const delta = now - this.lastFrameTimeStamp;
    this.lastFrameTimeStamp = now;
    const fps = 1 / delta * 1000;

    this.frames.push(fps);
    if (this.frames.length > 100) {
      this.frames.shift();
    }

    let min = Infinity;
    let max = Infinity;
    let sum = this.frames.reduce((acc, val) => {
      acc += val;
      min = Math.min(val, min);
      max = Math.max(val, max);
      return acc;
    });
    let mean = sum / this.frames.length;

    this.fps.textContent = `FPS: ${Math.round(mean)}`.trim();
  }
};
