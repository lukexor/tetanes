import { memory } from "tetanes-web/tetanes_web_bg";

let SCALE = 3;
let WIDTH = SCALE * 256;
let HEIGHT = SCALE * 240;
let FRAME_LEN = 4 * WIDTH * HEIGHT;

let drawCanvas;
let drawCtx;
let scaledCanvas
let scaledCtx;
let pixels;

export const setup = (state) => {
  WIDTH = state.nes.width();
  HEIGHT = state.nes.height();
  FRAME_LEN = state.nes.frame_len();

  // Set up Canvas
  drawCanvas = document.getElementById("tetanes-draw-canvas");
  drawCanvas.width = WIDTH;
  drawCanvas.height = HEIGHT;

  drawCtx = drawCanvas.getContext("2d");
  pixels = drawCtx.getImageData(0, 0, WIDTH, HEIGHT);

  // Scaled canvas
  scaledCanvas = document.getElementById("tetanes-canvas");
  scaledCanvas.width = WIDTH * SCALE;
  scaledCanvas.height = HEIGHT * SCALE;

  scaledCtx = scaledCanvas.getContext("2d");
  scaledCtx.scale(SCALE, SCALE);
};

// Render a frame
export const renderFrame = (nes) => {
  const framePtr = nes.frame();
  pixels.data.set(new Uint8ClampedArray(memory.buffer, framePtr, FRAME_LEN));
  drawCtx.putImageData(pixels, 0, 0);
  scaledCtx.drawImage(drawCanvas, 0, 0);
};
