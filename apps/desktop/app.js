const invoke = window.__TAURI__?.core?.invoke ?? (async () => {
  throw new Error("Tauri API недоступен — запусти через Deskcustom.app");
});

let currentRole = "server";
let pollTimer = null;
let updateCheckTimer = null;
let updateDismissed = false;
let checkIntervalSecs = 1800;

const els = {
  statusPill: document.getElementById("statusPill"),
  statusText: document.getElementById("statusText"),
  roleServer: document.getElementById("roleServer"),
  roleClient: document.getElementById("roleClient"),
  serverPanel: document.getElementById("serverPanel"),
  clientPanel: document.getElementById("clientPanel"),
  localIp: document.getElementById("localIp"),
  serverAddr: document.getElementById("serverAddr"),
  startBtn: document.getElementById("startBtn"),
  stopBtn: document.getElementById("stopBtn"),
  message: document.getElementById("message"),
  dpiScale: document.getElementById("dpiScale"),
  ewmaAlpha: document.getElementById("ewmaAlpha"),
  pollCap: document.getElementById("pollCap"),
  dpiVal: document.getElementById("dpiVal"),
  alphaVal: document.getElementById("alphaVal"),
  pollVal: document.getElementById("pollVal"),
  altShiftLocal: document.getElementById("altShiftLocal"),
  clipboardSync: document.getElementById("clipboardSync"),
  ctrlToCmd: document.getElementById("ctrlToCmd"),
  updateEnabled: document.getElementById("updateEnabled"),
  updateUrl: document.getElementById("updateUrl"),
  appVersion: document.getElementById("appVersion"),
  checkUpdateBtn: document.getElementById("checkUpdateBtn"),
  updateBanner: document.getElementById("updateBanner"),
  updateTitle: document.getElementById("updateTitle"),
  updateNotes: document.getElementById("updateNotes"),
  installUpdateBtn: document.getElementById("installUpdateBtn"),
  dismissUpdateBtn: document.getElementById("dismissUpdateBtn"),
  errorBanner: document.getElementById("errorBanner"),
  errorBannerText: document.getElementById("errorBannerText"),
  errorUpdateBtn: document.getElementById("errorUpdateBtn"),
  rtt: document.getElementById("rtt"),
  jitter: document.getElementById("jitter"),
  mouseCount: document.getElementById("mouseCount"),
  peerInfo: document.getElementById("peerInfo"),
  localScreenName: document.getElementById("localScreenName"),
  remoteScreenName: document.getElementById("remoteScreenName"),
  screenLocal: document.getElementById("screenLocal"),
  screenRemote: document.getElementById("screenRemote"),
};

function setRole(role) {
  currentRole = role;
  els.roleServer.classList.toggle("active", role === "server");
  els.roleClient.classList.toggle("active", role === "client");
  els.serverPanel.classList.toggle("hidden", role !== "server");
  els.clientPanel.classList.toggle("hidden", role !== "client");
  els.localScreenName.textContent = role === "server" ? "Windows PC" : "MacBook";
  els.remoteScreenName.textContent = role === "server" ? "MacBook" : "Windows PC";
}

async function saveConfig() {
  await invoke("save_ui_config", {
    role: currentRole,
    serverAddr: els.serverAddr.value.trim(),
    dpiScale: parseFloat(els.dpiScale.value),
    ewmaAlpha: parseFloat(els.ewmaAlpha.value),
    pollRateCapHz: parseInt(els.pollCap.value, 10),
    altShiftLocal: els.altShiftLocal.checked,
    clipboardSync: els.clipboardSync.checked,
    ctrlToCmd: els.ctrlToCmd.checked,
    updateEnabled: els.updateEnabled.checked,
    updateUrl: els.updateUrl.value.trim(),
  });
}

async function loadConfig() {
  const cfg = await invoke("get_ui_config");
  setRole(cfg.role);
  els.serverAddr.value = cfg.server_addr || "";
  els.localIp.textContent = cfg.server_display || cfg.local_ips?.[0] || "—";
  els.dpiScale.value = cfg.dpi_scale;
  els.ewmaAlpha.value = cfg.ewma_alpha;
  els.pollCap.value = cfg.poll_rate_cap_hz;
  els.altShiftLocal.checked = cfg.alt_shift_local;
  els.clipboardSync.checked = cfg.clipboard_sync;
  els.ctrlToCmd.checked = cfg.ctrl_to_cmd;
  els.updateEnabled.checked = cfg.update_enabled;
  els.updateUrl.value = cfg.update_url || "";
  els.appVersion.textContent = cfg.app_version || "—";
  updateSliderLabels();
}

function updateSliderLabels() {
  els.dpiVal.textContent = parseFloat(els.dpiScale.value).toFixed(1);
  els.alphaVal.textContent = parseFloat(els.ewmaAlpha.value).toFixed(2);
  els.pollVal.textContent = els.pollCap.value;
}

function showUpdateBanner(info) {
  if (updateDismissed) return;
  els.updateTitle.textContent = `Доступно обновление ${info.latest_version}`;
  els.updateNotes.textContent = info.notes || `Текущая версия: ${info.current_version}`;
  els.updateBanner.classList.remove("hidden");
}

function hideUpdateBanner() {
  els.updateBanner.classList.add("hidden");
}

async function runUpdateFlow() {
  els.message.textContent = "Скачиваем обновление…";
  await invoke("install_app_update");
}

async function checkUpdates(manual = false) {
  if (!els.updateEnabled.checked && !manual) return;
  try {
    const info = await invoke("check_app_update");
    if (info.available) {
      showUpdateBanner(info);
    } else if (manual) {
      els.message.textContent = info.error
        ? `Обновления: ${info.error}`
        : "Установлена последняя версия";
    }
  } catch (err) {
    if (manual) els.message.textContent = String(err);
  }
}

async function refreshStatus() {
  const st = await invoke("get_status");
  const running = st.running && st.phase === "running";

  els.statusPill.classList.toggle("running", running);
  els.statusPill.classList.toggle("error", st.phase === "error");
  els.statusText.textContent = running ? "Работает" : st.phase === "error" ? "Ошибка" : "Остановлено";
  els.message.textContent = st.message;
  els.startBtn.disabled = running;
  els.stopBtn.disabled = !running;

  els.rtt.textContent = st.rtt_ms > 0 ? `${st.rtt_ms.toFixed(1)} ms` : "— ms";
  els.jitter.textContent = st.jitter_ms > 0 ? `${st.jitter_ms.toFixed(1)} ms` : "— ms";
  els.mouseCount.textContent = `${st.mouse_sent} / ${st.mouse_recv}`;

  if (st.connected_peer) {
    els.peerInfo.textContent = `Подключено: ${st.connected_peer}`;
  } else {
    els.peerInfo.textContent = running ? "Ждём подключение клиента…" : "Клиент не подключён";
  }

  els.screenLocal.classList.toggle("active", !st.active_screen);
  els.screenRemote.classList.toggle("active", !!st.active_screen);

  if (st.suggest_update || st.phase === "error") {
    els.errorBannerText.textContent = st.message || "Попробуй обновить приложение";
    els.errorBanner.classList.remove("hidden");
    if (!updateDismissed) checkUpdates(false);
  } else {
    els.errorBanner.classList.add("hidden");
  }
}

async function startService() {
  await saveConfig();
  await invoke("start_service");
  await refreshStatus();
}

async function stopService() {
  await invoke("stop_service");
  await refreshStatus();
}

els.roleServer.addEventListener("click", () => { setRole("server"); saveConfig().catch(showError); });
els.roleClient.addEventListener("click", () => { setRole("client"); saveConfig().catch(showError); });

[els.dpiScale, els.ewmaAlpha, els.pollCap].forEach((el) => {
  el.addEventListener("input", updateSliderLabels);
  el.addEventListener("change", saveConfig);
});

[els.altShiftLocal, els.clipboardSync, els.ctrlToCmd, els.updateEnabled].forEach((el) => {
  el.addEventListener("change", saveConfig);
});
els.serverAddr.addEventListener("change", saveConfig);
els.updateUrl.addEventListener("change", saveConfig);

els.startBtn.addEventListener("click", () => startService().catch(showError));
els.stopBtn.addEventListener("click", () => stopService().catch(showError));
els.checkUpdateBtn.addEventListener("click", () => checkUpdates(true).catch(showError));
els.installUpdateBtn.addEventListener("click", () => runUpdateFlow().catch(showError));
els.errorUpdateBtn.addEventListener("click", () => runUpdateFlow().catch(showError));
els.dismissUpdateBtn.addEventListener("click", () => {
  updateDismissed = true;
  hideUpdateBanner();
});

function showError(err) {
  els.message.textContent = String(err);
  els.statusPill.classList.add("error");
}

loadConfig()
  .then(() => {
    refreshStatus();
    checkUpdates(false);
  })
  .catch(showError);

pollTimer = setInterval(() => refreshStatus().catch(() => {}), 1000);
updateCheckTimer = setInterval(() => checkUpdates(false).catch(() => {}), checkIntervalSecs * 1000);
