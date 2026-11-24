(() => {
  const api = window.windowShot;
  if (!api) {
    document.body.textContent = "WindowShot bridge failed to load.";
    return;
  }

  const elements = {
    windowSelect: document.getElementById("window-select"),
    windowMeta: document.getElementById("window-meta"),
    refresh: document.getElementById("btn-refresh"),
    capture: document.getElementById("btn-capture"),
    save: document.getElementById("btn-save"),
    status: document.getElementById("status"),
    preview: document.getElementById("preview"),
    empty: document.getElementById("empty-state"),
    previewSize: document.getElementById("preview-size"),
    toast: document.getElementById("toast"),
    source: document.getElementById("source-select"),
    shadowColor: document.getElementById("shadow-color"),
  };

  const sliders = {
    radius: document.getElementById("radius"),
    padding: document.getElementById("padding"),
    shadow_blur: document.getElementById("shadow-blur"),
    shadow_offset: document.getElementById("shadow-offset"),
    shadow_opacity: document.getElementById("shadow-opacity"),
  };

  const outputs = {
    radius: document.getElementById("radius-value"),
    padding: document.getElementById("padding-value"),
    shadow_blur: document.getElementById("shadow-blur-value"),
    shadow_offset: document.getElementById("shadow-offset-value"),
    shadow_opacity: document.getElementById("shadow-opacity-value"),
  };

  const STYLE_KEY = "window-shot-style-v1";
  let windows = [];
  let lastSuggestedName = "";
  let toastTimer = null;

  function showToast(message, isError = false) {
    elements.toast.textContent = message;
    elements.toast.classList.toggle("error", !!isError);
    elements.toast.classList.remove("hidden");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => elements.toast.classList.add("hidden"), 2800);
  }

  function setStatus(message, isError = false) {
    elements.status.textContent = message;
    elements.status.classList.toggle("error", !!isError);
  }

  function syncSliderOutput(key) {
    const value = Number(sliders[key].value);
    outputs[key].textContent = key === "shadow_opacity" ? `${value}%` : String(value);
  }

  function readStyle() {
    const hex = elements.shadowColor.value.replace("#", "");
    return {
      radius: Number(sliders.radius.value),
      padding: Number(sliders.padding.value),
      shadow_blur: Number(sliders.shadow_blur.value),
      shadow_offset: Number(sliders.shadow_offset.value),
      shadow_opacity: Number(sliders.shadow_opacity.value) / 100,
      shadow_color: [
        parseInt(hex.slice(0, 2), 16),
        parseInt(hex.slice(2, 4), 16),
        parseInt(hex.slice(4, 6), 16),
      ],
    };
  }

  function persistStyle() {
    try {
      localStorage.setItem(STYLE_KEY, JSON.stringify(readStyle()));
    } catch (_) {
      /* storage is optional */
    }
  }

  function restoreStyle() {
    try {
      const saved = JSON.parse(localStorage.getItem(STYLE_KEY) || "null");
      if (!saved) return;
      for (const key of Object.keys(sliders)) {
        if (typeof saved[key] === "number") sliders[key].value = String(saved[key]);
      }
      if (Array.isArray(saved.shadow_color) && saved.shadow_color.length === 3) {
        const hex = saved.shadow_color
          .map((channel) => Math.max(0, Math.min(255, Number(channel) || 0)).toString(16).padStart(2, "0"))
          .join("");
        elements.shadowColor.value = `#${hex}`;
      }
    } catch (_) {
      /* ignore malformed saved styles */
    }
  }

  function selectedWindow() {
    return windows.find((window) => window.hwnd === elements.windowSelect.value) || null;
  }

  function updateWindowMeta() {
    const window = selectedWindow();
    if (!window) {
      elements.windowMeta.textContent = windows.length ? "Select a window." : "No capturable windows found.";
      return;
    }
    elements.windowMeta.textContent =
      `${window.width}×${window.height} • DPI ${window.dpi} • PID ${window.pid} • ${window.hwnd}`;
  }

  async function refreshWindows() {
    setStatus("Refreshing windows…");
    try {
      const previous = elements.windowSelect.value;
      windows = await api.listWindows();
      elements.windowSelect.innerHTML = "";
      for (const window of windows) {
        const option = document.createElement("option");
        option.value = window.hwnd;
        option.textContent = `${window.title} — ${window.process}`;
        elements.windowSelect.appendChild(option);
      }
      if (previous && windows.some((window) => window.hwnd === previous)) {
        elements.windowSelect.value = previous;
      }
      updateWindowMeta();
      setStatus(windows.length ? `${windows.length} capturable windows.` : "No capturable windows found.");
    } catch (error) {
      setStatus(String(error), true);
      showToast(`Could not list windows: ${error}`, true);
    }
  }

  async function capture() {
    const window = selectedWindow();
    if (!window) {
      setStatus("Select a window first.", true);
      return;
    }

    elements.capture.disabled = true;
    setStatus("Capturing…");
    try {
      const result = await api.capture(window.hwnd, readStyle(), elements.source.value);
      elements.preview.src = result.data_url;
      elements.preview.hidden = false;
      elements.empty.hidden = true;
      elements.previewSize.textContent = `${result.width}×${result.height}`;
      elements.save.disabled = false;
      lastSuggestedName = result.suggested_name;
      setStatus(`Captured ${result.width}×${result.height}. Ready to save.`);
    } catch (error) {
      setStatus(String(error), true);
      showToast(`Capture failed: ${error}`, true);
    } finally {
      elements.capture.disabled = false;
    }
  }

  async function save() {
    elements.save.disabled = true;
    try {
      const path = await api.save(lastSuggestedName);
      if (path) {
        setStatus(`Saved to ${path}`);
        showToast("Screenshot saved.");
      } else {
        setStatus("Save cancelled.");
      }
    } catch (error) {
      setStatus(String(error), true);
      showToast(`Save failed: ${error}`, true);
    } finally {
      elements.save.disabled = false;
    }
  }

  for (const key of Object.keys(sliders)) {
    sliders[key].addEventListener("input", () => {
      syncSliderOutput(key);
      persistStyle();
    });
  }
  elements.shadowColor.addEventListener("input", persistStyle);
  elements.windowSelect.addEventListener("change", updateWindowMeta);
  elements.refresh.addEventListener("click", refreshWindows);
  elements.capture.addEventListener("click", capture);
  elements.save.addEventListener("click", save);

  document.addEventListener("keydown", (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      capture();
    }
  });

  api.bindWindowControls();
  restoreStyle();
  for (const key of Object.keys(sliders)) syncSliderOutput(key);
  refreshWindows();
})();
