/**
 * window.windowShot - renderer API for the WindowShot Tauri backend.
 * Mirrors the preload-style bridge pattern used by CmdDeck.
 */
(function (global) {
  const bridge = global.tauriTrayBridge;
  if (!bridge) throw new Error("tauri-tray-bridge.js must load first");

  const { invoke, listen, bindWindowControls, minimizeWindow, closeWindow } = bridge;

  function onEvent(event, callback) {
    let unlisten = null;
    let cancelled = false;
    listen(event, callback).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }

  global.windowShot = {
    listWindows: () => invoke("list_windows"),
    capture: (hwnd, style, source) => invoke("capture_window", { hwnd, style, source }),
    save: (suggestedName) => invoke("save_last", { suggestedName: suggestedName || null }),
    getSettings: () => bridge.getSettings(),
    setSettings: (partial) => bridge.setSettings(partial),
    bindWindowControls: () => bindWindowControls(document),
    minimizeWindow,
    closeWindow,
    onToast: (callback) => onEvent("toast", callback),
    onSettingsChanged: (callback) => onEvent("settings:changed", callback),
  };
})(typeof window !== "undefined" ? window : globalThis);
