import { Nes } from "tetanes-web";
import * as events from "./events.js";
import * as render from "./render.js";
import * as audio from "./audio.js";

const state = {
  nes: Nes.new(),
  animationId: 0,
  emulationLoop: () => {
    state.nes.clock_frame();
    render.renderFrame(state.nes);
    audio.playAudio(state.nes);
    state.animationId = requestAnimationFrame(state.emulationLoop);
  },
};

render.setup(state);
events.setup(state);
audio.setup(state);
