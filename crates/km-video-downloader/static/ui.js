// KM Video Downloader — the small amount of script the page needs.
//
// **Almost all of it is about one htmx behavior**: htmx does not swap a non-2xx response. A handler
// that answers 400 with a perfectly good explanation puts nothing on the page at all, so a failure
// is indistinguishable from a button that does nothing. Every refusal this program can produce is
// worded for a person — no folder set, a fetch already running, a list that could not be written —
// and none of it would ever be seen without this.

(function () {
  "use strict";

  const tray = () => document.getElementById("toasts");

  /** Shows one message until it is dismissed or times out. */
  function toast(text) {
    const host = tray();
    if (!host) return;
    const box = document.createElement("div");
    box.className = "toast";
    box.setAttribute("role", "status");
    box.textContent = text;
    box.addEventListener("click", () => box.remove());
    host.appendChild(box);
    // Long enough to read a sentence, which is what these are.
    setTimeout(() => box.remove(), 12000);
  }

  /** What a failed request should say, preferring the server's own words. */
  function reason(detail) {
    const response = detail && detail.xhr;
    if (!response) return "The page could not reach this program.";
    const body = (response.responseText || "").trim();
    // The handlers answer with a sentence, not a stack trace. If one somehow returns HTML, do not
    // paste a document into a toast.
    if (body && body.length < 400 && !body.startsWith("<")) return body;
    if (response.status === 0) return "This program stopped answering.";
    return `Something went wrong (${response.status}).`;
  }

  // **On `document`, not on `document.body`, and that distinction is load-bearing here.** Setting
  // the output folder answers with the whole page and swaps it in with `hx-target="body"
  // hx-swap="outerHTML"` — which replaces the body element itself, and takes every listener
  // attached to it. Bound to the body, these survived exactly until somebody chose a folder, after
  // which every refusal was silent again and the button looked dead. htmx's events bubble to
  // `document`, which is not replaced by anything.
  document.addEventListener("htmx:responseError", (event) => {
    toast(reason(event.detail));
  });

  document.addEventListener("htmx:sendError", () => {
    toast("This program stopped answering. Is it still running?");
  });

  // Closing the folder browser puts the page back the way it loaded, which the server has nothing
  // to say about — so it is done here rather than through a route that would only ever return the
  // same fixed button. Delegated from `document` for the reason above.
  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-close-browser]");
    if (!button) return;
    event.preventDefault();
    const host = document.getElementById("listing");
    if (!host) return;
    host.replaceChildren();
    const open = document.createElement("button");
    open.className = "link";
    open.textContent = "Browse this computer…";
    open.setAttribute("hx-get", "/browse");
    open.setAttribute("hx-target", "#listing");
    open.setAttribute("hx-swap", "innerHTML");
    host.appendChild(open);
    // Newly created nodes are not wired up until htmx is told about them.
    window.htmx.process(host);
  });

  // Exposed so a fragment can raise one without a round trip of its own.
  window.kmToast = toast;
})();
