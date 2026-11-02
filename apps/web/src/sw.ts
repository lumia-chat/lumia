// The service worker: Web Push arrives here. The payload is the
// instance's generic notice (architecture 15.5); the worker cannot reach
// the encrypted store, so it shows the notice as is and opens the app on
// a tap, which decrypts as usual. Nothing is cached here.
/// <reference lib="webworker" />
const sw = self as unknown as ServiceWorkerGlobalScope;

sw.addEventListener("install", () => void sw.skipWaiting());
sw.addEventListener("activate", (event) => (event as ExtendableEvent).waitUntil(sw.clients.claim()));

sw.addEventListener("push", (event) => {
  const push = event as PushEvent;
  let notice: { title?: string; body?: string } = {};
  try {
    notice = (push.data?.json() as typeof notice) ?? {};
  } catch {
    /* not JSON: still a notice */
  }
  push.waitUntil(
    sw.registration.showNotification(notice.title || "Lumia", {
      body: notice.body || "New message",
      tag: "lumia-new-message",
      icon: "/brand/lumia-original-icon.png",
    }),
  );
});

sw.addEventListener("notificationclick", (event) => {
  const click = event as NotificationEvent;
  click.notification.close();
  click.waitUntil(
    sw.clients.matchAll({ type: "window", includeUncontrolled: true }).then(async (list) => {
      const open = list.find((c) => "focus" in c);
      if (open) return open.focus();
      return sw.clients.openWindow("/app?place=home");
    }),
  );
});
