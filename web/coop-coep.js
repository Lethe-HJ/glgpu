// GitHub Pages cannot emit COOP/COEP. This same-origin service worker adds the
// same isolation headers as `cargo xtask serve` so `/?threads=1` can use
// SharedArrayBuffer. It is not registered when the document is already
// cross-origin isolated, and it forwards responses that already carry the
// headers, so the local server keeps owning that path.
const ISOLATION_RELOAD_KEY = "glgpu-pages-isolation-reload";

if (typeof window === "undefined") {
  self.addEventListener("install", (event) => {
    event.waitUntil(self.skipWaiting());
  });

  self.addEventListener("activate", (event) => {
    event.waitUntil(self.clients.claim());
  });

  self.addEventListener("fetch", (event) => {
    const request = event.request;
    if (request.cache === "only-if-cached" && request.mode !== "same-origin") {
      return;
    }

    event.respondWith(
      withIsolationHeaders(request).catch((error) => {
        console.error("GLGPU isolation fetch failed", error);
        return new Response("isolation fetch failed\n", {
          status: 502,
          headers: { "Content-Type": "text/plain; charset=utf-8" },
        });
      }),
    );
  });
} else {
  const scriptUrl = document.currentScript && document.currentScript.src;
  window.glgpuIsolationReady = preparePagesIsolation(scriptUrl);
}

async function withIsolationHeaders(request) {
  const response = await fetch(request);
  if (response.status === 0) {
    return response;
  }

  if (
    response.headers.get("Cross-Origin-Opener-Policy") === "same-origin" &&
    response.headers.get("Cross-Origin-Embedder-Policy") === "require-corp"
  ) {
    return response;
  }

  const headers = new Headers(response.headers);
  headers.set("Cross-Origin-Opener-Policy", "same-origin");
  headers.set("Cross-Origin-Embedder-Policy", "require-corp");
  if (!headers.has("Cross-Origin-Resource-Policy")) {
    headers.set("Cross-Origin-Resource-Policy", "same-origin");
  }

  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

async function preparePagesIsolation(scriptUrl) {
  const wantsThreads = new URLSearchParams(location.search).get("threads") === "1";
  if (!wantsThreads) {
    return { reloading: false };
  }

  if (window.crossOriginIsolated) {
    storageRemove(ISOLATION_RELOAD_KEY);
    stripRetryParam();
    return { reloading: false };
  }

  if (!window.isSecureContext || !scriptUrl || !navigator.serviceWorker) {
    console.warn("GLGPU Pages isolation is unavailable; using ST");
    return { reloading: false };
  }

  let registration;
  try {
    registration = await navigator.serviceWorker.register(scriptUrl, {
      updateViaCache: "none",
    });
  } catch (error) {
    console.warn("GLGPU could not register the Pages isolation service worker", error);
    return { reloading: false };
  }

  // A controller on this document does not make it isolated. Headers apply on
  // the next navigation, so the first controlled load still has to reload.
  // The query param stops a loop when storage is unavailable.
  if (alreadyRetried()) {
    console.warn("GLGPU Pages isolation service worker did not enable cross-origin isolation; using ST");
    return { reloading: false };
  }

  if (registration.active || navigator.serviceWorker.controller) {
    return reloadForIsolation();
  }

  const controlled = await new Promise((resolve) => {
    const timeout = setTimeout(() => resolve(false), 5000);
    navigator.serviceWorker.addEventListener(
      "controllerchange",
      () => {
        clearTimeout(timeout);
        resolve(true);
      },
      { once: true },
    );
  });

  if (!controlled && !navigator.serviceWorker.controller && !registration.active) {
    console.warn("GLGPU Pages isolation service worker did not take control; using ST");
    return { reloading: false };
  }

  return reloadForIsolation();
}

function reloadForIsolation() {
  storageSet(ISOLATION_RELOAD_KEY, "1");
  const url = new URL(location.href);
  url.searchParams.set("glgpu_isolation", "1");
  console.info("GLGPU reloading once to apply COOP/COEP for the MT build");
  location.replace(url.href);
  return { reloading: true };
}

function alreadyRetried() {
  return new URLSearchParams(location.search).get("glgpu_isolation") === "1"
    || storageGet(ISOLATION_RELOAD_KEY) === "1";
}

function stripRetryParam() {
  const url = new URL(location.href);
  if (!url.searchParams.has("glgpu_isolation")) {
    return;
  }
  url.searchParams.delete("glgpu_isolation");
  const next = url.pathname + url.search + url.hash;
  history.replaceState(null, "", next);
}

function storageGet(key) {
  try {
    return sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function storageSet(key, value) {
  try {
    sessionStorage.setItem(key, value);
  } catch {
    // Private mode may reject storage. The reload still proceeds once.
  }
}

function storageRemove(key) {
  try {
    sessionStorage.removeItem(key);
  } catch {
    // Ignore storage failures; isolation does not depend on this key.
  }
}
