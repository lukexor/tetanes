import { Nes } from "tetanes-web";
import { memory } from "tetanes-web/tetanes_web_bg.wasm";

const WIDTH = 256;
const HEIGHT = 240;
const CLIP_TOP = 8;
const CLIP_BOTTOM = 8;
const CANVAS_ID = "view";
const BACK_CANVAS_ID = "backView";

type Rom = {
  name: string;
  filename: string;
};

const HOMEBREW_ROMS: Rom[] = [
  {
    name: "Alter Ego",
    filename: "alter_ego.nes",
  },
  {
    name: "AO",
    filename: "ao_demo.nes",
  },
  {
    name: "Assimilate",
    filename: "assimilate.nes",
  },
  {
    name: "Blade Buster",
    filename: "blade_buster.nes",
  },
  {
    name: "From Below",
    filename: "from_below.nes",
  },
  {
    name: "Lan Master",
    filename: "lan_master.nes",
  },
  {
    name: "Streemerz",
    filename: "streemerz.nes",
  },
];

const getElement = (id: string): null | HTMLElement => {
  const el = document.getElementById(id);
  if (!el) {
    console.error(`${id} not found`);
  }
  return el;
};

let canvas = <HTMLCanvasElement>getElement(CANVAS_ID);
let backCanvas = <HTMLCanvasElement>getElement(BACK_CANVAS_ID);

// blitting a single texture can be faster than drawing a 2d image on canvas.
const setupWebgl = (
  width: number,
  height: number,
  scale: number
): null | WebGLRenderingContext => {
  const FRAG_SHADER = `
    precision mediump float;
    varying vec2 v_texcoord;
    uniform sampler2D u_sampler;
    void main() {
        gl_FragColor = vec4( texture2D( u_sampler, vec2( v_texcoord.s, v_texcoord.t ) ).rgb, 1.0 );
    }
  `;
  const VERT_SHADER = `
    attribute vec2 a_position;
    attribute vec2 a_texcoord;
    uniform mat4 u_matrix;
    varying vec2 v_texcoord;
    void main() {
        gl_Position = u_matrix * vec4( a_position, 0.0, 1.0 );
        v_texcoord = a_texcoord;
    }
  `;

  const ortho = (
    left: number,
    right: number,
    bottom: number,
    top: number
  ): number[] => {
    // prettier-ignore
    const m = [
      1.0, 0.0, 0.0, 0.0,
      0.0, 1.0, 0.0, 0.0,
      0.0, 0.0, 1.0, 0.0,
      0.0, 0.0, 0.0, 1.0,
    ];
    m[0 * 4 + 0] = 2.0 / (right - left);
    m[1 * 4 + 1] = 2.0 / (top - bottom);
    m[3 * 4 + 0] = ((right + left) / (right - left)) * -1.0;
    m[3 * 4 + 1] = ((top + bottom) / (top - bottom)) * -1.0;
    return m;
  };

  const newCanvas = <HTMLCanvasElement>document.createElement("canvas");
  newCanvas.id = CANVAS_ID;
  canvas.parentNode?.replaceChild(newCanvas, canvas);
  canvas = newCanvas;
  canvas.width = scale * width;
  canvas.height = scale * height;

  const webgl = canvas.getContext("webgl");

  if (!webgl) {
    console.error("WebGL rendering context not found.");
    return null;
  }

  const vertShader = webgl.createShader(webgl.VERTEX_SHADER);
  const fragShader = webgl.createShader(webgl.FRAGMENT_SHADER);

  if (!vertShader || !fragShader) {
    console.error("WebGL shader creation failed.");
    return null;
  }

  webgl.shaderSource(vertShader, VERT_SHADER);
  webgl.shaderSource(fragShader, FRAG_SHADER);
  webgl.compileShader(vertShader);
  webgl.compileShader(fragShader);

  if (!webgl.getShaderParameter(vertShader, webgl.COMPILE_STATUS)) {
    console.error(
      "WebGL vertex shader compilation failed:",
      webgl.getShaderInfoLog(vertShader)
    );
    return null;
  }
  if (!webgl.getShaderParameter(fragShader, webgl.COMPILE_STATUS)) {
    console.error(
      "WebGL fragment shader compilation failed:",
      webgl.getShaderInfoLog(fragShader)
    );
    return null;
  }

  const program = webgl.createProgram();
  if (!program) {
    console.error("WebGL program creation failed.");
    return null;
  }

  webgl.attachShader(program, vertShader);
  webgl.attachShader(program, fragShader);
  webgl.linkProgram(program);
  if (!webgl.getProgramParameter(program, webgl.LINK_STATUS)) {
    console.error("WebGL program linking failed!");
    return null;
  }

  webgl.useProgram(program);

  var vertex_attr = webgl.getAttribLocation(program, "a_position");
  var texcoord_attr = webgl.getAttribLocation(program, "a_texcoord");

  webgl.enableVertexAttribArray(vertex_attr);
  webgl.enableVertexAttribArray(texcoord_attr);

  var sampler_uniform = webgl.getUniformLocation(program, "u_sampler");
  webgl.uniform1i(sampler_uniform, 0);

  var matrix = ortho(0.0, width, height, 0.0);
  var matrix_uniform = webgl.getUniformLocation(program, "u_matrix");
  webgl.uniformMatrix4fv(matrix_uniform, false, matrix);

  var texture = webgl.createTexture();
  webgl.bindTexture(webgl.TEXTURE_2D, texture);
  webgl.texImage2D(
    webgl.TEXTURE_2D,
    0,
    webgl.RGBA,
    width,
    width,
    0,
    webgl.RGBA,
    webgl.UNSIGNED_BYTE,
    new Uint8Array(width * width * 4)
  );
  webgl.texParameteri(
    webgl.TEXTURE_2D,
    webgl.TEXTURE_MAG_FILTER,
    webgl.NEAREST
  );
  webgl.texParameteri(
    webgl.TEXTURE_2D,
    webgl.TEXTURE_MIN_FILTER,
    webgl.NEAREST
  );

  var vertex_buffer = webgl.createBuffer();
  webgl.bindBuffer(webgl.ARRAY_BUFFER, vertex_buffer);
  // prettier-ignore
  var vertices = [
    0.0, 0.0,
    0.0, height,
    width, 0.0,
    width, height,
  ];
  webgl.bufferData(
    webgl.ARRAY_BUFFER,
    new Float32Array(vertices),
    webgl.STATIC_DRAW
  );
  webgl.vertexAttribPointer(vertex_attr, 2, webgl.FLOAT, false, 0, 0);

  var texcoord_buffer = webgl.createBuffer();
  webgl.bindBuffer(webgl.ARRAY_BUFFER, texcoord_buffer);
  // prettier-ignore
  var texcoords = [
    0.0, 0.0,
    0.0, height / width,
    1.0, 0.0,
    1.0, height / width,
  ];
  webgl.bufferData(
    webgl.ARRAY_BUFFER,
    new Float32Array(texcoords),
    webgl.STATIC_DRAW
  );
  webgl.vertexAttribPointer(texcoord_attr, 2, webgl.FLOAT, false, 0, 0);

  var index_buffer = webgl.createBuffer();
  webgl.bindBuffer(webgl.ELEMENT_ARRAY_BUFFER, index_buffer);
  // prettier-ignore
  var indices = [
    0, 1, 2,
    2, 3, 1,
  ];
  webgl.bufferData(
    webgl.ELEMENT_ARRAY_BUFFER,
    new Uint16Array(indices),
    webgl.STATIC_DRAW
  );

  webgl.clear(webgl.COLOR_BUFFER_BIT);
  webgl.enable(webgl.DEPTH_TEST);
  // TODO: Add top/bottom clip
  const viewRect = [0, 0, canvas.width, canvas.height] as const;
  webgl.viewport(...viewRect);

  return webgl;
};

const setupWeb2d = (
  width: number,
  height: number,
  scale: number
): null | {
  canvasCtx: CanvasRenderingContext2D;
  backCanvasCtx: CanvasRenderingContext2D;
  imageData: ImageData;
  imageBuffer: Uint8Array;
} => {
  const newCanvas = <HTMLCanvasElement>document.createElement("canvas");
  newCanvas.id = CANVAS_ID;
  canvas.parentNode?.replaceChild(newCanvas, canvas);
  canvas = newCanvas;
  canvas.width = scale * width;
  canvas.height = scale * height;
  backCanvas.width = width;
  backCanvas.height = height;

  const canvasCtx = canvas.getContext("2d");
  const backCanvasCtx = backCanvas.getContext("2d");

  if (!canvasCtx || !backCanvasCtx) {
    console.error("Web2d rendering context not found.");
    return null;
  }

  const clip = new Path2D();
  const viewRect = [
    0,
    CLIP_TOP,
    width,
    height - (CLIP_TOP + CLIP_BOTTOM),
  ] as const;
  clip.rect(...viewRect);
  canvasCtx.scale(scale, scale);
  canvasCtx.clip(clip);

  const imageData = backCanvasCtx.createImageData(width, height);
  if (!imageData) {
    console.error("imageData creation failed.");
    return null;
  }

  const imageBuffer = new Uint8Array(imageData.data.buffer);

  return { canvasCtx, backCanvasCtx, imageData, imageBuffer };
};

class State {
  nes: Nes;
  webgl: null | WebGLRenderingContext = null;
  canvasCtx: null | CanvasRenderingContext2D = null;
  backCanvasCtx: null | CanvasRenderingContext2D = null;
  fps: Fps;
  audioEnabled: boolean;
  keybinds: string[];
  loaded = false;
  paused = true;

  scale = 2;
  width = this.scale * WIDTH;
  height = this.scale * HEIGHT;
  imageData: null | ImageData = null;
  imageBuffer: null | Uint8Array = null;
  deltaTime = 0;
  lastFrameTime = 0;

  sampleRate = 48000;
  bufferSize = 800;
  maxDelta = 0.02;
  audioCtx: null | AudioContext = null;
  emptyBuffers: AudioBuffer[] = [];
  buffered = 0.0;
  nextStartTime = 0.0;

  constructor() {
    this.nes = Nes.new(this.sampleRate, this.bufferSize, this.maxDelta);
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
    this.sampleRate = this.nes.sample_rate();
    this.bufferSize = this.nes.buffer_capacity();
    this.emptyBuffers = [];

    this.webgl = setupWebgl(WIDTH, HEIGHT, this.scale);
    if (!this.webgl) {
      console.log("No WebGL. Falling back to Web2D");

      const web2d = setupWeb2d(WIDTH, HEIGHT, this.scale);
      if (!web2d) {
        console.error("Web2d creation failed");
        handleError("Failed to setup canvas");
        return;
      }
      this.canvasCtx = web2d.canvasCtx;
      this.backCanvasCtx = web2d.backCanvasCtx;
      this.imageData = web2d.imageData;
      this.imageBuffer = web2d.imageBuffer;
    }
  }

  loadRom(data: Uint8Array) {
    this.nes.load_rom(data);
    this.pause(false);

    if (!this.loaded) {
      const AudioContext = window.AudioContext || window.webkitAudioContext;
      this.audioCtx = new AudioContext({ sampleRate: this.sampleRate });
      this.emptyBuffers = [];
    }
    this.loaded = true;

    const loadRomLabel = getElement("load-rom-label");
    if (loadRomLabel) {
      loadRomLabel.textContent = "Change ROM";
    }
    clearError();
  }

  setSound(enabled: boolean) {
    this.audioEnabled = enabled;
    this.nes.set_sound(enabled);
  }

  handleEvent(key: string, pressed: boolean, repeat: boolean) {
    if (this.loaded && this.keybinds.includes(key)) {
      this.nes.handle_event(key, pressed, repeat);
      return true;
    }
    return false;
  }

  setScale(scale: number) {
    this.scale = scale;
    this.width = this.scale * WIDTH;
    this.height = this.scale * HEIGHT;

    if (this.webgl) {
      this.webgl = setupWebgl(WIDTH, HEIGHT, this.scale);
      if (!this.webgl) {
        console.error("WebGL creation failed");
        return;
      }
    } else if (this.canvasCtx) {
      const web2d = setupWeb2d(WIDTH, HEIGHT, this.scale);
      if (!web2d) {
        console.error("Web2d creation failed");
        return;
      }
      this.canvasCtx = web2d.canvasCtx;
      this.imageData = web2d.imageData;
      this.imageBuffer = web2d.imageBuffer;
    }
  }

  clock() {
    const now = performance.now();
    this.deltaTime = now - this.lastFrameTime;
    this.lastFrameTime = now;
    this.fps.tick();
    let secondsToRun = Math.min(
      Math.max(this.deltaTime / 1000.0, 0.0),
      1.0 / 60.0
    );
    this.nes.clock_seconds(secondsToRun);
  }

  pause(paused: boolean) {
    this.paused = paused;
    this.nes.pause(paused);
  }

  render() {
    const frameLen = this.nes.frame_len();
    const framePtr = this.nes.frame();
    const buffer = new Uint8Array(memory.buffer, framePtr, frameLen);
    if (this.webgl) {
      this.webgl.texSubImage2D(
        this.webgl.TEXTURE_2D,
        0,
        0,
        0,
        WIDTH,
        HEIGHT,
        this.webgl.RGBA,
        this.webgl.UNSIGNED_BYTE,
        buffer
      );
      this.webgl.drawElements(
        this.webgl.TRIANGLES,
        6,
        this.webgl.UNSIGNED_SHORT,
        0
      );
    } else if (this.canvasCtx && this.imageBuffer && this.imageData) {
      this.imageBuffer.set(buffer);
      this.backCanvasCtx!.putImageData(this.imageData, 0, 0);
      this.canvasCtx.drawImage(backCanvas, 0, 0);
    } else {
      console.error("WebGL and Web2D failed to render");
    }
  }

  playAudio() {
    if (this.audioEnabled && this.audioCtx) {
      const samplesPtr = this.nes.samples();
      const samples = new Float32Array(
        memory.buffer,
        samplesPtr,
        this.bufferSize
      );

      let audioBuffer: AudioBuffer;
      if (this.emptyBuffers.length) {
        audioBuffer = this.emptyBuffers.pop()!;
      } else {
        audioBuffer = this.audioCtx.createBuffer(
          1,
          this.bufferSize,
          this.sampleRate
        );
      }

      audioBuffer.getChannelData(0).set(samples);

      const node = this.audioCtx.createBufferSource();
      node.connect(this.audioCtx.destination);
      node.buffer = audioBuffer;
      node.onended = () => {
        this.emptyBuffers.push(audioBuffer);
      };

      const latency = 0.032; // Two frames worth
      this.buffered =
        this.nextStartTime - (this.audioCtx.currentTime + latency);
      const start = Math.max(
        this.nextStartTime || 0,
        this.audioCtx.currentTime + latency
      );
      node.start(start);
      this.nextStartTime = start + this.bufferSize / this.sampleRate;
    }
  }
}

class Fps {
  fpsCounter: HTMLElement;
  frames: number[] = [];
  lastFrameTime: number = 0.0;

  constructor() {
    this.fpsCounter = getElement("fps")!;
    this.frames = [];
    this.lastFrameTime = performance.now();
  }

  tick() {
    const now = performance.now();
    const delta = now - this.lastFrameTime;
    this.lastFrameTime = now;

    const fps = (1 / delta) * 1000;
    this.frames.push(fps);
    if (this.frames.length > 100) {
      this.frames.shift();
    }

    let min = Infinity;
    let max = Infinity;
    const sum = this.frames.reduce((acc, val) => {
      acc += val;
      min = Math.min(val, min);
      max = Math.max(val, max);
      return acc;
    });
    const mean = sum / this.frames.length;

    this.fpsCounter.textContent = `FPS: ${Math.round(mean)}`.trim();
  }
}

const setupRomLoading = (state: State) => {
  const loadRom = getElement("load-rom");
  if (!loadRom) {
    return;
  }

  loadRom.addEventListener("click", (evt: MouseEvent) => {
    state.pause(true);
    (<HTMLElement>evt.currentTarget).blur();
  });

  loadRom.addEventListener(
    "change",
    (evt: Event) => {
      const reader = new FileReader();
      const files = (<HTMLInputElement>evt.currentTarget).files;
      if (reader && files?.length && files[0]) {
        reader.readAsArrayBuffer(files[0]);
        reader.onload = () => {
          const data = new Uint8Array(<ArrayBuffer>reader.result);
          state.loadRom(data);
        };
      } else {
        console.error("failed to load rom");
      }
    },
    false
  );
};

const setupEventHandling = (state: State) => {
  for (let i = 1; i <= 3; ++i) {
    const scale = getElement(`scale${i}`);
    if (scale) {
      scale.addEventListener(
        "click",
        (evt: MouseEvent) => {
          state.setScale(i);
          (<HTMLElement>evt.currentTarget).blur();
        },
        false
      );
    }
  }

  const toggleAudio = getElement("toggle-audio");
  if (toggleAudio) {
    toggleAudio.addEventListener(
      "click",
      (evt: MouseEvent) => {
        if (state.audioEnabled) {
          toggleAudio.textContent = "Unmute";
          state.setSound(false);
        } else {
          toggleAudio.textContent = "Mute";
          state.setSound(true);
        }
        (<HTMLElement>evt.currentTarget).blur();
      },
      false
    );
  }

  const togglePause = getElement("toggle-pause");
  if (togglePause) {
    togglePause.addEventListener(
      "click",
      (evt: MouseEvent) => {
        if (state.paused && state.loaded) {
          togglePause.textContent = "Pause";
          state.pause(false);
        } else {
          togglePause.textContent = "UnPause";
          state.pause(true);
        }
        (<HTMLElement>evt.currentTarget).blur();
      },
      false
    );
  }

  window.addEventListener("keydown", (evt: KeyboardEvent) => {
    let handled = state.handleEvent(evt.key, true, evt.repeat);
    if (handled) {
      evt.preventDefault();
    }
  });
  window.addEventListener("keyup", (evt: KeyboardEvent) => {
    let handled = state.handleEvent(evt.key, false, evt.repeat);
    if (handled) {
      evt.preventDefault();
    }
  });
};

const setupHomebrewRoms = (state: State) => {
  const homebrewMenu = getElement("homebrew-menu");
  const homebrewList = getElement("homebrew-list");
  const loadHomebrew = getElement("load-homebrew");
  const homebrewClose = getElement("homebrew-close");

  if (!homebrewMenu || !homebrewList || !loadHomebrew || !homebrewClose) {
    return;
  }

  const openMenu = (evt: MouseEvent) => {
    state.pause(true);
    homebrewMenu.classList.remove("hidden");
    (<HTMLElement>evt.currentTarget).blur();
  };
  const closeMenu = (evt: MouseEvent) => {
    if (state.loaded) {
      state.pause(false);
    }
    homebrewMenu.classList.add("hidden");
    (<HTMLElement>evt.currentTarget).blur();
  };

  loadHomebrew.addEventListener("click", openMenu, false);
  homebrewClose.addEventListener("click", closeMenu, false);

  for (let rom of HOMEBREW_ROMS) {
    const button = document.createElement("button");
    button.textContent = rom.name;
    button.addEventListener("click", async (evt: MouseEvent) => {
      closeMenu(evt);
      try {
        const res = await fetch(`/roms/${rom.filename}`);
        const data = new Uint8Array(await res.arrayBuffer());
        state.loadRom(data);
      } catch (err) {
        if (err instanceof Error) {
          console.error(err.message);
          handleError("Failed to load ROM.");
        }
      }
    });

    homebrewList.appendChild(button);
  }
};

const mainLoop = (state: State) => {
  if (!state.paused) {
    state.clock();
    state.playAudio();
    state.render();
  }
  window.requestAnimationFrame(() => {
    mainLoop(state);
  });
};

const clearError = () => handleError("");

const handleError = (error: string) => {
  const errorMsg = getElement("error");
  if (errorMsg) {
    errorMsg.textContent = error;
  }
};

const initialize = () => {
  Nes.init();
  const state = new State();

  setupRomLoading(state);
  setupEventHandling(state);
  setupHomebrewRoms(state);

  window.requestAnimationFrame(() => {
    mainLoop(state);
  });
};

initialize();
