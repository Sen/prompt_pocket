document.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-copy-target]");

  if (!button) {
    return;
  }

  const target = document.querySelector(button.getAttribute("data-copy-target"));
  const text = target?.innerText?.trim();

  if (!text) {
    return;
  }

  try {
    await navigator.clipboard.writeText(text);
    button.textContent = "已复制";
    window.setTimeout(() => {
      button.textContent = "复制";
    }, 1200);
  } catch {
    button.textContent = "复制失败";
    window.setTimeout(() => {
      button.textContent = "复制";
    }, 1200);
  }
});

document.addEventListener("htmx:beforeRequest", (event) => {
  const form = event.target.closest(".tool-form");

  if (!form) {
    return;
  }

  const target = document.querySelector(form.getAttribute("hx-target"));

  if (!target) {
    return;
  }

  target.replaceChildren();
  clearRequestId(form);
  setFormLoading(form, true);
});

document.addEventListener("htmx:afterRequest", (event) => {
  const form = event.target.closest(".tool-form");

  if (form) {
    setFormLoading(form, false);
  }
});

document.addEventListener("htmx:sendError", (event) => {
  const form = event.target.closest(".tool-form");

  if (form) {
    setFormLoading(form, false);
  }
});

function setFormLoading(form, loading) {
  const button = form.querySelector('button[type="submit"]');

  if (!button) {
    return;
  }

  if (button.dataset.initiallyDisabled === undefined) {
    button.dataset.initiallyDisabled = button.disabled ? "true" : "false";
  }

  button.disabled = loading || button.dataset.initiallyDisabled === "true";
  button.setAttribute("aria-busy", loading ? "true" : "false");
}

function clearRequestId(form) {
  const mode = new FormData(form).get("mode");

  if (!mode) {
    return;
  }

  const requestId = document.querySelector(`#request-${CSS.escape(mode)}`);

  if (requestId) {
    requestId.textContent = "";
  }
}
