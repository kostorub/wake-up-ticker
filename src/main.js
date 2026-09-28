'use strict';

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const el = (id) => document.getElementById(id);

const dom = {
  status: el('status'),
  statusEyebrow: el('status-eyebrow'),
  statusTime: el('status-time'),
  statusDetail: el('status-detail'),
  btnTest: el('btn-test'),
  btnToggle: el('btn-toggle'),
  btnCancelSnooze: el('btn-cancel-snooze'),
  snoozeButtons: el('snooze-buttons'),
  snoozeRow: el('snooze-row'),
  presets: el('presets'),
  minutes: el('minutes'),
  minutesError: el('minutes-error'),
  minutesPreview: el('minutes-preview'),
  title: el('title'),
  body: el('body'),
  sound: el('sound'),
  snoozeMinutes: el('snoozeMinutes'),
  weekdaysOnly: el('weekdaysOnly'),
  quietEnabled: el('quietEnabled'),
  quietRange: el('quiet-range'),
  quietFrom: el('quietFrom'),
  quietTo: el('quietTo'),
  autoStart: el('autoStart'),
  showTimeInMenuBar: el('showTimeInMenuBar'),
  overlay: el('overlay'),
  overlayOptions: el('overlay-options'),
  overlaySeconds: el('overlaySeconds'),
  btnPreview: el('btn-preview'),
  menubarToggle: el('menubar-toggle'),
  version: el('version'),
  btnReveal: el('btn-reveal'),
  btnQuit: el('btn-quit'),
  toast: el('saved-toast'),
};

let config = null;
let suppressEvents = false;
let toastTimer = null;

/* ------------------------------------------------------------- helpers */

function parseMinutes(text) {
  const tokens = String(text)
    .split(/[^0-9]+/)
    .filter(Boolean);
  const values = [];
  for (const token of tokens) {
    const value = Number.parseInt(token, 10);
    if (!Number.isInteger(value) || value < 0 || value > 59) {
      return { error: `"${token}" is not a minute between 0 and 59.` };
    }
    values.push(value);
  }
  return { values: [...new Set(values)].sort((a, b) => a - b) };
}

function formatMinutes(minutes) {
  return minutes.map((m) => String(m).padStart(2, '0')).join(', ');
}

function describeSchedule(minutes) {
  if (minutes.length === 0) return 'No reminders scheduled.';
  const times = minutes.map((m) => `:${String(m).padStart(2, '0')}`).join(' and ');
  const perHour = minutes.length === 1 ? 'Once an hour' : `${minutes.length}× an hour`;
  return `${perHour}, at ${times} past the hour.`;
}

function showToast(message = 'Saved') {
  dom.toast.textContent = message;
  dom.toast.classList.add('visible');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => dom.toast.classList.remove('visible'), 1200);
}

function clockOf(timestamp) {
  const date = new Date(timestamp);
  return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

/* -------------------------------------------------------------- render */

function renderStatus(status) {
  const paused = !status.enabled;
  dom.status.classList.toggle('is-paused', paused || Boolean(status.snoozedUntil));
  dom.btnToggle.textContent = paused ? 'Resume' : 'Pause';
  dom.btnCancelSnooze.classList.toggle('hidden', !status.snoozedUntil);
  dom.snoozeRow.classList.toggle('disabled', paused);

  if (paused) {
    dom.statusEyebrow.textContent = 'Reminders';
    dom.statusTime.textContent = 'Paused';
    dom.statusDetail.textContent = 'Nothing will fire until you resume.';
    return;
  }

  if (!status.hasTimes) {
    dom.statusEyebrow.textContent = 'Reminders';
    dom.statusTime.textContent = '--:--';
    dom.statusDetail.textContent = 'Add at least one minute below to get started.';
    return;
  }

  if (status.snoozedUntil) {
    dom.statusEyebrow.textContent = 'Snoozed';
    dom.statusTime.textContent = clockOf(status.snoozedUntil);
    dom.statusDetail.textContent = status.nextFireClock
      ? `Reminders resume at ${status.nextFireClock}.`
      : 'Reminders resume after the snooze.';
    return;
  }

  dom.statusEyebrow.textContent = 'Next reminder';
  dom.statusTime.textContent = status.nextFireClock || '--:--';
  let detail = status.nextFireDelay || '';
  if (status.inQuietHours) detail += ' · quiet hours are on right now';
  else if (status.offDay) detail += ' · weekends are off';
  dom.statusDetail.textContent = detail;
}

function renderPresets(minutes) {
  const value = minutes.join(',');
  for (const chip of dom.presets.querySelectorAll('.chip')) {
    chip.classList.toggle('active', chip.dataset.minutes === value);
  }
}

function renderConfig(next) {
  suppressEvents = true;
  dom.minutes.value = formatMinutes(next.minutes);
  dom.minutesPreview.textContent = describeSchedule(next.minutes);
  dom.minutesError.classList.add('hidden');
  dom.title.value = next.title;
  dom.body.value = next.body;
  dom.sound.checked = next.sound;
  dom.snoozeMinutes.value = next.snoozeMinutes;
  dom.weekdaysOnly.checked = next.weekdaysOnly;
  dom.quietEnabled.checked = next.quietHours.enabled;
  dom.quietFrom.value = next.quietHours.from;
  dom.quietTo.value = next.quietHours.to;
  dom.quietRange.classList.toggle('disabled', !next.quietHours.enabled);
  dom.autoStart.checked = next.autoStart;
  dom.showTimeInMenuBar.checked = next.showTimeInMenuBar;
  dom.overlay.checked = next.overlay;
  dom.overlaySeconds.value = next.overlaySeconds;
  dom.overlayOptions.classList.toggle('disabled', !next.overlay);
  renderPresets(next.minutes);
  suppressEvents = false;
}

/* -------------------------------------------------------------- wiring */

/**
 * The backend takes a whole config, not a patch — merging here keeps the
 * settings window the single source of truth for what the user typed.
 */
async function save(patch, { toast = true } = {}) {
  if (suppressEvents) return;
  config = { ...config, ...patch };
  const status = await invoke('save_config', { config });
  renderStatus(status);
  renderPresets(config.minutes);
  if (toast) showToast();
}

function commitMinutes() {
  const parsed = parseMinutes(dom.minutes.value);
  if (parsed.error) {
    dom.minutesError.textContent = parsed.error;
    dom.minutesError.classList.remove('hidden');
    return;
  }
  dom.minutesError.classList.add('hidden');
  dom.minutesPreview.textContent = describeSchedule(parsed.values);
  renderPresets(parsed.values);
  save({ minutes: parsed.values });
}

function bind() {
  dom.btnTest.addEventListener('click', () => invoke('test_notification'));
  dom.btnToggle.addEventListener('click', () =>
    save({ enabled: !config.enabled }, { toast: false }),
  );
  dom.btnCancelSnooze.addEventListener('click', async () => {
    renderStatus(await invoke('clear_snooze'));
  });

  dom.presets.addEventListener('click', (event) => {
    const chip = event.target.closest('.chip');
    if (!chip) return;
    const values = chip.dataset.minutes.split(',').map(Number);
    dom.minutes.value = formatMinutes(values);
    dom.minutesError.classList.add('hidden');
    dom.minutesPreview.textContent = describeSchedule(values);
    renderPresets(values);
    save({ minutes: values });
  });

  // Live validation while typing, but only persist on blur / Enter.
  dom.minutes.addEventListener('input', () => {
    const parsed = parseMinutes(dom.minutes.value);
    if (parsed.error) {
      dom.minutesError.textContent = parsed.error;
      dom.minutesError.classList.remove('hidden');
    } else {
      dom.minutesError.classList.add('hidden');
      dom.minutesPreview.textContent = describeSchedule(parsed.values);
    }
  });
  dom.minutes.addEventListener('change', commitMinutes);
  dom.minutes.addEventListener('blur', commitMinutes);
  dom.minutes.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') dom.minutes.blur();
  });

  dom.title.addEventListener('change', () => save({ title: dom.title.value }));
  dom.body.addEventListener('change', () => save({ body: dom.body.value }));
  dom.sound.addEventListener('change', () => save({ sound: dom.sound.checked }));
  dom.snoozeMinutes.addEventListener('change', () => {
    const value = Number.parseInt(dom.snoozeMinutes.value, 10);
    save({ snoozeMinutes: Number.isInteger(value) ? value : config.snoozeMinutes });
  });
  dom.weekdaysOnly.addEventListener('change', () =>
    save({ weekdaysOnly: dom.weekdaysOnly.checked }),
  );
  dom.autoStart.addEventListener('change', () => save({ autoStart: dom.autoStart.checked }));
  dom.showTimeInMenuBar.addEventListener('change', () =>
    save({ showTimeInMenuBar: dom.showTimeInMenuBar.checked }),
  );

  const saveQuiet = () =>
    save({
      quietHours: {
        enabled: dom.quietEnabled.checked,
        from: dom.quietFrom.value,
        to: dom.quietTo.value,
      },
    });
  dom.quietEnabled.addEventListener('change', () => {
    dom.quietRange.classList.toggle('disabled', !dom.quietEnabled.checked);
    saveQuiet();
  });
  dom.quietFrom.addEventListener('change', saveQuiet);
  dom.quietTo.addEventListener('change', saveQuiet);

  dom.overlay.addEventListener('change', () => {
    dom.overlayOptions.classList.toggle('disabled', !dom.overlay.checked);
    save({ overlay: dom.overlay.checked });
  });
  dom.overlaySeconds.addEventListener('change', () => {
    const value = Number.parseInt(dom.overlaySeconds.value, 10);
    save({ overlaySeconds: Number.isInteger(value) ? value : config.overlaySeconds });
  });
  dom.btnPreview.addEventListener('click', () => invoke('preview_overlay'));

  dom.btnReveal.addEventListener('click', () => invoke('reveal_config'));
  dom.btnQuit.addEventListener('click', () => invoke('quit_app'));

  // The tray menu can change the same state, so mirror pushes from the backend.
  listen('state-changed', (event) => {
    config = event.payload.config;
    renderConfig(config);
    renderStatus(event.payload.status);
  });
}

function buildSnoozeButtons(presets) {
  dom.snoozeButtons.innerHTML = '';
  for (const minutes of presets) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn btn-small';
    button.textContent = minutes >= 60 ? `${minutes / 60} h` : `${minutes} m`;
    button.addEventListener('click', async () => {
      renderStatus(await invoke('snooze', { minutes }));
    });
    dom.snoozeButtons.appendChild(button);
  }

  const skip = document.createElement('button');
  skip.type = 'button';
  skip.className = 'btn btn-small';
  skip.textContent = 'Skip next';
  skip.addEventListener('click', async () => {
    renderStatus(await invoke('skip_next'));
  });
  dom.snoozeButtons.appendChild(skip);
}

async function boot() {
  const state = await invoke('get_state');
  config = state.config;
  buildSnoozeButtons(state.snoozePresets);
  if (state.platform !== 'macos') dom.menubarToggle.classList.add('hidden');
  dom.version.textContent = `Version ${state.version}`;
  renderConfig(config);
  renderStatus(state.status);
  bind();
}

boot().catch((error) => {
  dom.statusDetail.textContent = `Could not load settings: ${error}`;
});
