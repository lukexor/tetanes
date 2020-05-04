import { Nes } from "tetanes-web";
import * as events from "./events.js";
import * as render from "./render.js";
import * as audio from "./audio.js";

const state = {
  nes: Nes.new(),
  emulationLoop: () => {
    events.fps.render();
    events.handleEvents(state);
    state.nes.clock_frame();
    render.renderFrame(state.nes);
    audio.playAudio(state.nes);
    requestAnimationFrame(state.emulationLoop);
  },
};

render.setup(state);
events.setup(state);
audio.setup(state);
