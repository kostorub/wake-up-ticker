'use strict';

const { invoke } = window.__TAURI__.core;

const RING_CIRCUMFERENCE = 339.292; // 2 * pi * 54, matching overlay.css

const SUGGESTIONS = [
  'Roll your shoulders back, then look at something far away.',
  'Stand up straight and reach for the ceiling.',
  'Twenty squats. Right now. Nobody is watching.',
  'Walk to the window and let your eyes refocus.',
  'Neck rolls — slowly, both directions.',
  'Stretch your wrists and shake out your hands.',
  'Touch your toes, or get as close as you honestly can.',
  'Refill your water while you are up.',
];

const dom = {
  title: document.getElementById('title'),
  body: document.getElementById('body'),
  countdown: document.getElementById('countdown'),
  ring: document.getElementById('ring'),
  suggestion: document.getElementById('suggestion'),
  done: document.getElementById('btn-done'),
  snooze: document.getElementById('btn-snooze'),
  skip: document.getElementById('btn-skip'),
};

let closing = false;

function dismiss(action) {
  if (closing) return;
  closing = true;
  invoke('dismiss_overlay', { action }).catch(() => {
    // If the backend is unreachable the failsafe in Rust still closes us.
  });
}

function startCountdown(seconds) {
  let remaining = seconds;
  dom.countdown.textContent = String(remaining);
  dom.ring.style.strokeDasharray = String(RING_CIRCUMFERENCE);
  dom.ring.style.strokeDashoffset = '0';

  const tick = () => {
    remaining -= 1;
    if (remaining <= 0) {
      dom.countdown.textContent = '0';
      dom.ring.style.strokeDashoffset = String(RING_CIRCUMFERENCE);
      clearInterval(timer);
      dismiss('done');
      return;
    }
    dom.countdown.textContent = String(remaining);
    dom.ring.style.strokeDashoffset = String(
      RING_CIRCUMFERENCE * (1 - remaining / seconds),
    );
  };

  const timer = setInterval(tick, 1000);
}

async function boot() {
  const state = await invoke('overlay_state');

  dom.title.textContent = state.title;
  dom.body.textContent = state.body;
  dom.snooze.textContent = `Snooze ${state.snoozeMinutes} min`;
  dom.suggestion.textContent =
    SUGGESTIONS[Math.floor(Math.random() * SUGGESTIONS.length)];
  if (state.allowSkip) dom.skip.classList.remove('hidden');

  dom.done.addEventListener('click', () => dismiss('done'));
  dom.snooze.addEventListener('click', () => dismiss('snooze'));
  dom.skip.addEventListener('click', () => dismiss('skip'));

  // Always available, even with Skip hidden: a screen-covering window must
  // never be something you cannot get out of.
  window.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') dismiss('skip');
  });

  dom.done.focus();
  startCountdown(state.seconds);
}

boot().catch((error) => {
  dom.body.textContent = `Could not start the break: ${error}`;
  dom.countdown.textContent = '!';
});
