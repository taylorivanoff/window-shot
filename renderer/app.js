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
    picker: document.getElementById("btn-picker"),
    capture: document.getElementById("btn-capture"),
    importButton: document.getElementById("btn-import"),
    importFile: document.getElementById("import-file"),
    save: document.getElementById("btn-save"),
    status: document.getElementById("status"),
    preview: document.getElementById("preview"),
    previewStage: document.getElementById("preview-stage"),
    empty: document.getElementById("empty-state"),
    previewSize: document.getElementById("preview-size"),
    toast: document.getElementById("toast"),
    source: document.getElementById("source-select"),
    shadowColor: document.getElementById("shadow-color"),
    backgroundTransparent: document.getElementById("background-transparent"),
    backgroundColor: document.getElementById("background-color"),
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
  const RESTYLE_DELAY_MS = 90;
  const HEX_COLOR = /^#[0-9a-f]{6}$/i;

  let windows = [];
  let lastSuggestedName = "";
  let toastTimer = null;
  let hasStyledImage = false;
  let restyleTimer = null;
  let restyleToken = 0;

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

  function hexToRgb(value) {
    const hex = value.replace("#", "");
    return [
      parseInt(hex.slice(0, 2), 16),
      parseInt(hex.slice(2, 4), 16),
      parseInt(hex.slice(4, 6), 16),
    ];
  }

  function rgbToHex(rgb) {
    return `#${rgb
      .map((channel) => Math.max(0, Math.min(255, Number(channel) || 0)).toString(16).padStart(2, "0"))
      .join("")}`;
  }

  function readStyle() {
    return {
      radius: Number(sliders.radius.value),
      padding: Number(sliders.padding.value),
      shadow_blur: Number(sliders.shadow_blur.value),
      shadow_offset: Number(sliders.shadow_offset.value),
      shadow_opacity: Number(sliders.shadow_opacity.value) / 100,
      shadow_color: hexToRgb(elements.shadowColor.value),
      background_color: elements.backgroundTransparent.checked
        ? null
        : hexToRgb(elements.backgroundColor.value),
    };
  }

  function syncBackgroundUI() {
    const transparent = elements.backgroundTransparent.checked;
    elements.backgroundColor.disabled = transparent;
    elements.previewStage.classList.toggle("has-background", !transparent);
    if (transparent) {
      elements.previewStage.style.removeProperty("--preview-bg");
    } else {
      elements.previewStage.style.setProperty("--preview-bg", elements.backgroundColor.value);
    }
  }

  function persistStyle() {
    try {
      localStorage.setItem(
        STYLE_KEY,
        JSON.stringify({
          ...readStyle(),
          background_transparent: elements.backgroundTransparent.checked,
          background_hex: elements.backgroundColor.value,
        }),
      );
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
        elements.shadowColor.value = rgbToHex(saved.shadow_color);
      }
      if (typeof saved.background_transparent === "boolean") {
        elements.backgroundTransparent.checked = saved.background_transparent;
      }
      if (typeof saved.background_hex === "string" && HEX_COLOR.test(saved.background_hex)) {
        elements.backgroundColor.value = saved.background_hex;
      } else if (Array.isArray(saved.background_color) && saved.background_color.length === 3) {
        elements.backgroundColor.value = rgbToHex(saved.background_color);
        elements.backgroundTransparent.checked = false;
      }
    } catch (_) {
      /* ignore malformed saved styles */
    } finally {
      syncBackgroundUI();
    }
  }

  function applyResult(result, options = {}) {
    elements.preview.src = result.data_url;
    elements.preview.hidden = false;
    elements.empty.hidden = true;
    elements.previewSize.textContent = `${result.width}×${result.height}`;
    elements.save.disabled = false;
    lastSuggestedName = result.suggested_name;
    hasStyledImage = true;

    if (options.status !== undefined) {
      setStatus(options.status);
    } else if (result.file_path) {
      setStatus(`Captured ${result.width}×${result.height}. Opened in Photos.`);
      showToast("Screenshot saved and opened in Photos.");
    } else {
      setStatus(`Captured ${result.width}×${result.height}. Ready to save.`);
    }
  }

  function scheduleRestyle() {
    if (!hasStyledImage) return;
    clearTimeout(restyleTimer);
    const token = ++restyleToken;
    restyleTimer = setTimeout(async () => {
      try {
        const result = await api.restyle(readStyle());
        if (token !== restyleToken) return;
        applyResult(result, { status: `Styled preview ${result.width}×${result.height}.` });
      } catch (error) {
        if (token !== restyleToken) return;
        setStatus(String(error), true);
      }
    }, RESTYLE_DELAY_MS);
  }

  function readFileAsDataUrl(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.onerror = () => reject(reader.error || new Error("Could not read the image."));
      reader.readAsDataURL(file);
    });
  }

  async function loadImage(file, suggestedName) {
    if (!file) return;
    if (file.type && !file.type.startsWith("image/")) {
      showToast("Choose an image file.", true);
      return;
    }

    setStatus("Importing image…");
    try {
      const dataUrl = await readFileAsDataUrl(file);
      const result = await api.importImage(dataUrl, readStyle(), suggestedName || null);
      applyResult(result, { status: `Imported ${result.width}×${result.height}. Ready to save.` });
      showToast("Image imported and styled.");
      elements.previewStage.focus({ preventScroll: true });
    } catch (error) {
      setStatus(String(error), true);
      showToast(`Import failed: ${error}`, true);
    }
  }

  function imageFileFromClipboard(event) {
    const isImage = (type) => !type || type.startsWith("image/");
    const items = Array.from(event.clipboardData?.items || []);
    const item = items.find((entry) => entry.kind === "file" && isImage(entry.type));
    const file = item?.getAsFile();
    if (file) return file;
    return (
      Array.from(event.clipboardData?.files || []).find((entry) => isImage(entry.type)) || null
    );
  }

  function syncSliderOutput(key) {
    const value = Number(sliders[key].value);
    outputs[key].textContent = key === "shadow_opacity" ? `${value}%` : String(value);
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
      applyResult(await api.capture(window.hwnd, readStyle(), elements.source.value));
    } catch (error) {
      setStatus(String(error), true);
      showToast(`Capture failed: ${error}`, true);
    } finally {
      elements.capture.disabled = false;
    }
  }

  async function startPicker() {
    setStatus("Move over a window and click to capture. Esc cancels.");
    try {
      await api.startPicker(readStyle(), elements.source.value);
    } catch (error) {
      setStatus(String(error), true);
      showToast(`Picker failed: ${error}`, true);
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
      scheduleRestyle();
    });
  }
  elements.shadowColor.addEventListener("input", () => {
    persistStyle();
    scheduleRestyle();
  });
  elements.backgroundTransparent.addEventListener("change", () => {
    syncBackgroundUI();
    persistStyle();
    scheduleRestyle();
  });
  elements.backgroundColor.addEventListener("input", () => {
    syncBackgroundUI();
    persistStyle();
    scheduleRestyle();
  });
  elements.windowSelect.addEventListener("change", updateWindowMeta);
  elements.refresh.addEventListener("click", refreshWindows);
  elements.picker.addEventListener("click", startPicker);
  elements.capture.addEventListener("click", capture);
  elements.save.addEventListener("click", save);
  elements.importButton.addEventListener("click", () => elements.importFile.click());
  elements.importFile.addEventListener("change", (event) => {
    const file = event.target.files?.[0];
    if (file) loadImage(file, file.name);
    event.target.value = "";
  });

  document.addEventListener("paste", (event) => {
    const file = imageFileFromClipboard(event);
    if (!file) return;
    event.preventDefault();
    loadImage(file, null);
  });

  for (const type of ["dragenter", "dragover"]) {
    elements.previewStage.addEventListener(type, (event) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
      elements.previewStage.classList.add("drag-over");
    });
  }
  for (const type of ["dragleave", "dragend"]) {
    elements.previewStage.addEventListener(type, () => {
      elements.previewStage.classList.remove("drag-over");
    });
  }
  elements.previewStage.addEventListener("drop", (event) => {
    event.preventDefault();
    event.stopPropagation();
    elements.previewStage.classList.remove("drag-over");
    const file = event.dataTransfer?.files?.[0];
    if (file) loadImage(file, file.name);
  });
  document.addEventListener("dragover", (event) => event.preventDefault());
  document.addEventListener("drop", (event) => event.preventDefault());

  api.onPickerResult(applyResult);
  api.onToast((payload) => {
    const message = typeof payload === "string" ? payload : payload?.message || String(payload);
    showToast(message, Boolean(payload?.error));
  });

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
