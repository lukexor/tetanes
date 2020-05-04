import { memory } from "tetanes-web/tetanes_web_bg";

let SAMPLE_RATE = 48000;
let playTime = 0;

// Process audio
const AudioContext = window.AudioContext || window.webkitAudioContext;
let audioCtx;
const emptyBuffers = [];

export const setup = (state) => {
  SAMPLE_RATE = state.nes.sample_rate();
  audioCtx = new AudioContext({ sampleRate: SAMPLE_RATE });
};

export const playAudio = (nes) => {
  const samplesLen = nes.samples_len();
  const samplesPtr = nes.samples();
  const samples = new Float32Array(memory.buffer, samplesPtr, samplesLen);
  const audioBuffer = audioCtx.createBuffer(1, 4096, SAMPLE_RATE);
  audioBuffer.copyToChannel(samples, 0, 0);
  const audioSource = audioCtx.createBufferSource();
  audioSource.buffer = audioBuffer;
  audioSource.connect(audioCtx.destination);
  audioSource.onended = function() {
    emptyBuffers.push(audioBuffer);
  };

  const latency = (audioCtx.outputLatency ||  audioCtx.baseLatency);
  const buffered = playTime - audioCtx.currentTime + latency;
  const playTimestamp = Math.max(audioCtx.currentTime + latency, playTime);
  audioSource.start(playTimestamp);
  playTime = playTimestamp + samplesLen / SAMPLE_RATE;
  nes.clear_samples();
  return buffered;
};
