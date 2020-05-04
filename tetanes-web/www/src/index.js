import { Nes } from "tetanes-web";
import * as events from "./events.js";
import * as render from "./render.js";
import * as audio from "./audio.js";

Nes.init();
const state = {
  nes: Nes.new(),
  animationId: 0,
  emulationLoop: () => {
    events.fps.render();
    events.handleEvents(state);
    state.nes.clock_frame();
    render.renderFrame(state.nes);
    audio.playAudio(state.nes);
    state.animationId = requestAnimationFrame(state.emulationLoop);
  },
};

events.setup(state);
render.setup(state);
audio.setup(state);
