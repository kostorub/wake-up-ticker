'use strict';

const { invoke } = window.__TAURI__.core;

const RING_CIRCUMFERENCE = 339.292; // 2 * pi * 54, matching overlay.css

const dom = {
  title: document.getElementById('title'),
  countdown: document.getElementById('countdown'),
  ring: document.getElementById('ring'),
  done: document.getElementById('btn-done'),
};

let closing = false;

function dismiss() {
  if (closing) return;
  closing = true;
  invoke('dismiss_overlay').catch(() => {
    // If the backend is unreachable the failsafe in Rust still closes us.
  });
}

function startCountdown(seconds) {
  let remaining = seconds;
  dom.countdown.textContent = String(remaining);
  dom.ring.style.strokeDashoffset = '0';

  const timer = setInterval(() => {
    remaining -= 1;
    if (remaining <= 0) {
      dom.countdown.textContent = '0';
      dom.ring.style.strokeDashoffset = String(RING_CIRCUMFERENCE);
      clearInterval(timer);
      dismiss();
      return;
    }
    dom.countdown.textContent = String(remaining);
    dom.ring.style.strokeDashoffset = String(
      RING_CIRCUMFERENCE * (1 - remaining / seconds),
    );
  }, 1000);
}

async function boot() {
  const state = await invoke('overlay_state');
  dom.title.textContent = state.title;
  dom.done.addEventListener('click', dismiss);

  // Deliberately undocumented on screen, but always live: a window covering
  // every display must never be something you cannot get out of.
  window.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') dismiss();
  });

  dom.done.focus();
  startCountdown(state.seconds);
}

boot().catch(() => {
  // Show something actionable rather than a stuck blank screen.
  dom.title.textContent = 'Time to stand up';
  dom.countdown.textContent = '—';
  dom.done.addEventListener('click', dismiss);
});
