import { Nes } from "tetanes-web";
import { memory } from "tetanes-web/tetanes_web_bg.wasm";

let state;

class State {
  constructor(p5) {
    this.nes = Nes.new();
    this.p5 = p5;
    this.events = [];
    this.fps = new Fps();
    this.audioEnabled = true;
    this.keybinds = [
      "Escape",
      "Enter",
      "Shift",
      "a",
      "s",
      "z",
      "x",
      "ArrowUp",
      "ArrowDown",
      "ArrowLeft",
      "ArrowRight",
    ];
    this.setScale(2);
  }

  handleEvents() {
    this.events.splice(0).forEach((e) => {
      this.nes.handle_event(e.key, e.pressed, e.repeat);
    });
  }

  setScale(scale) {
    this.scale = scale;
    this.width = this.nes.width() * this.scale;
    this.height = this.nes.height() * this.scale;
    this.imageData = this.p5.drawingContext.createImageData(
      this.nes.width(),
      this.nes.height()
    );
    this.image = this.p5.createGraphics(this.nes.width(), this.nes.height());
  }

  clock() {
    this.fps.tick();
    this.nes.clock_frame();
  }

  paused() {
    return this.nes.paused();
  }

  render() {
    const frameLen = this.nes.frame_len();
    const framePtr = this.nes.frame();
    this.imageData.data.set(
      new Uint8ClampedArray(memory.buffer, framePtr, frameLen)
    );
    this.image.drawingContext.putImageData(this.imageData, 0, 0);
    this.p5.image(
      this.image,
      0,
      0,
      this.p5.pixelDensity() * this.p5.width,
      this.p5.pixelDensity() * this.p5.height
    );
  }

  setupAudio() {
    const AudioContext = window.AudioContext || window.webkitAudioContext;
    this.sampleRate = this.nes.sample_rate();
    this.audioCtx = new AudioContext({ sampleRate: this.sampleRate });
    this.audioTime = 0;
  }

  toggleAudio() {
    this.audioEnabled = !this.audioEnabled;
  }

  playAudio() {
    if (!this.audioEnabled) {
      this.nes.clear_samples();
      return;
    }

    const samplesLen = this.nes.samples_len();
    if (samplesLen > 0) {
      const samplesPtr = this.nes.samples();
      const samples = new Float32Array(memory.buffer, samplesPtr, samplesLen);
      const audioBuffer = this.audioCtx.createBuffer(
        1,
        samplesLen,
        this.sampleRate
      );
      audioBuffer.copyToChannel(samples, 0, 0);
      const audioSource = this.audioCtx.createBufferSource();
      audioSource.buffer = audioBuffer;
      audioSource.connect(this.audioCtx.destination);

      const latency = (this.audioCtx.outputLatency ||  this.audioCtx.baseLatency);
      const buffered = this.audioTime - this.audioCtx.currentTime + latency;
      const playTimestamp = Math.max(this.audioCtx.currentTime + latency, this.audioTime);
      audioSource.start(playTimestamp);
      this.audioTime = playTimestamp + samplesLen / this.sampleRate;

      this.nes.clear_samples();
    }
  }

  addEvent(e) {
    this.events.push(e);
  }
}

class Fps {
  constructor() {
    this.fps = document.getElementById("fps");
    this.frames = [];
    this.lastFrameTimeStamp = performance.now();
  }

  tick() {
    const now = performance.now();
    const delta = now - this.lastFrameTimeStamp;
    this.lastFrameTimeStamp = now;
    const fps = (1 / delta) * 1000;

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
}

const container = document.getElementById("p5-container");
const sketch = (p5) => {
  p5.disableFriendlyErrors = true;
  p5.setup = function () {
    Nes.init();
    state = new State(p5);
    p5.createCanvas(state.width, state.height);
    p5.background(33);
    document.getElementById("load-rom").addEventListener("click", function () {
      state.nes.pause(true);
      this.blur();
    });

    document.getElementById("load-rom").addEventListener(
      "change",
      function () {
        const reader = new FileReader();
        const files = this.files;
        if (reader && files.length) {
          reader.readAsArrayBuffer(files[0]);
          reader.onload = () => {
            const data = new Uint8Array(reader.result);
            state = new State(p5);
            state.nes.load_rom(data);
            state.setupAudio();
            p5.loop();
            document.getElementById("load-rom-label").textContent =
              "Change ROM";
          };
        }
      },
      false
    );

    for (let i = 1; i <= 3; ++i) {
      document.getElementById(`scale${i}`).addEventListener(
        "click",
        function () {
          state.setScale(i);
          p5.resizeCanvas(state.width, state.height);
          container.style.width = state.width + "px";
          container.style.height = state.height + "px";
          p5.background(33);
          p5.redraw();
          this.blur();
        },
        false
      );
    }

    document.getElementById("toggle-audio").addEventListener(
      "click",
      function () {
        state.toggleAudio();
        if (state.audioEnabled) {
          document.getElementById("toggle-audio").textContent = "Mute";
        } else {
          document.getElementById("toggle-audio").textContent = "Unmute";
        }
        this.blur();
      },
      false
    );

    p5.noLoop();
  };

  p5.draw = function () {
    state.handleEvents();
    if (!state.paused()) {
      state.clock();
      state.render();
      state.playAudio();
    }
  };

  p5.keyPressed = function (e) {
    if (state.keybinds.includes(e.key)) {
      state.addEvent({ key: e.key, pressed: true, repeat: e.repeat });
      return false;
    }
  };

  p5.keyReleased = function (e) {
    if (state.keybinds.includes(e.key)) {
      state.addEvent({ key: e.key, pressed: false, repeat: e.repeat });
      return false;
    }
  };
};

const P5 = new window.p5(sketch, container);
