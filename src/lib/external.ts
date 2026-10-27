import { api } from "./api";

const EXTERNAL = /^https:\/\//i;

function openExternal(url: string) {
  void api.openExternal(url).catch(() => {});
}

function externalTarget(event: MouseEvent): string | null {
  const path = event.composedPath();
  for (const node of path) {
    if (node instanceof HTMLAnchorElement && node.href) {
      return EXTERNAL.test(node.href) ? node.href : null;
    }
  }
  return null;
}

/** Ссылки и window.open уходят в системный браузер, а не в WebView. */
export function installExternalLinks() {
  document.addEventListener(
    "click",
    (event) => {
      const url = externalTarget(event);
      if (!url) return;
      event.preventDefault();
      event.stopPropagation();
      openExternal(url);
    },
    true,
  );

  document.addEventListener(
    "auxclick",
    (event) => {
      const url = externalTarget(event);
      if (!url) return;
      event.preventDefault();
      event.stopPropagation();
      openExternal(url);
    },
    true,
  );

  window.open = ((url?: string | URL | null) => {
    if (url) {
      const text = String(url);
      if (EXTERNAL.test(text)) openExternal(text);
    }
    return null;
  }) as typeof window.open;
}
