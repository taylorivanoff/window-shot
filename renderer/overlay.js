(() => {
  const api = window.windowShot;
  const canvas = document.getElementById("overlay");
  const hint = document.getElementById("hint");
  const context = canvas.getContext("2d");
  const idleHint = hint.textContent;

  let bounds = { left: 0, top: 0, width: 0, height: 0 };
  let windows = [];
  let hovered = null;
  let scale = window.devicePixelRatio || 1;
  let busy = false;

  function resize() {
    scale = window.devicePixelRatio || 1;
    canvas.width = Math.ceil(window.innerWidth * scale);
    canvas.height = Math.ceil(window.innerHeight * scale);
    draw();
  }

  function toLocal(rect) {
    return {
      x: (rect.left - bounds.left) / scale,
      y: (rect.top - bounds.top) / scale,
      width: rect.width / scale,
      height: rect.height / scale,
    };
  }

  function findWindowAt(clientX, clientY) {
    const screenX = bounds.left + clientX * scale;
    const screenY = bounds.top + clientY * scale;
    for (const window of windows) {
      if (
        screenX >= window.left &&
        screenX <= window.left + window.width &&
        screenY >= window.top &&
        screenY <= window.top + window.height
      ) {
        return window;
      }
    }
    return null;
  }

  function drawLabel(rect, window) {
    const label = `${window.title} — ${window.process}`;
    context.font = '12px "Segoe UI", system-ui, sans-serif';
    const padding = 9;
    const labelWidth = Math.min(context.measureText(label).width + padding * 2, rect.width);
    const labelHeight = 22;
    const x = rect.x;
    const y = rect.y >= labelHeight + 4 ? rect.y - labelHeight - 2 : rect.y + 2;

    context.fillStyle = "rgba(0, 120, 212, 0.95)";
    context.fillRect(x, y, labelWidth, labelHeight);
    context.fillStyle = "#ffffff";
    context.fillText(label, x + padding, y + 15, labelWidth - padding * 2);
  }

  function draw() {
    context.setTransform(scale, 0, 0, scale, 0, 0);
    context.clearRect(0, 0, window.innerWidth, window.innerHeight);

    context.fillStyle = "rgba(0, 0, 0, 0.42)";
    context.fillRect(0, 0, window.innerWidth, window.innerHeight);

    context.lineWidth = 1;
    context.strokeStyle = "rgba(255, 255, 255, 0.18)";
    for (const window of windows) {
      const rect = toLocal(window);
      context.strokeRect(rect.x + 0.5, rect.y + 0.5, rect.width - 1, rect.height - 1);
    }

    if (hovered) {
      const rect = toLocal(hovered);
      context.clearRect(rect.x, rect.y, rect.width, rect.height);
      context.fillStyle = "rgba(255, 255, 255, 0.05)";
      context.fillRect(rect.x, rect.y, rect.width, rect.height);
      context.lineWidth = 2;
      context.strokeStyle = "#60cdff";
      context.strokeRect(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2);
      drawLabel(rect, hovered);
    }
  }

  async function refresh() {
    if (!api) {
      hint.textContent = "WindowShot bridge failed to load.";
      return;
    }

    hovered = null;
    busy = false;
    hint.textContent = idleHint;
    hint.classList.remove("busy");

    try {
      bounds = await api.overlayBounds();
      windows = await api.listWindows();
    } catch (error) {
      hint.textContent = `Could not load windows: ${error}`;
      return;
    }

    resize();
  }

  window.addEventListener("resize", resize);

  window.addEventListener("mousemove", (event) => {
    if (busy) return;
    const next = findWindowAt(event.clientX, event.clientY);
    if (next?.hwnd !== hovered?.hwnd) {
      hovered = next;
      draw();
    }
  });

  window.addEventListener("click", async (event) => {
    if (busy) return;
    const target = findWindowAt(event.clientX, event.clientY);
    if (!target) return;

    busy = true;
    hint.textContent = "Capturing and opening in Photos…";
    hint.classList.add("busy");
    try {
      await api.pickWindow(target.hwnd);
    } catch (error) {
      hint.textContent = `Capture failed: ${error}`;
      hint.classList.remove("busy");
      setTimeout(() => {
        busy = false;
        hint.textContent = idleHint;
      }, 2200);
    }
  });

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      api?.cancelPicker();
    }
  });

  window.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    api?.cancelPicker();
  });

  api?.onPickerStart(refresh);
  refresh();
})();
