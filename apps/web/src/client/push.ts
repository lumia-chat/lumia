// Web Push on the page side (architecture 15.5): register the service
// worker, subscribe with the instance's VAPID key, hand the subscription
// to the instance through the worker client. Best effort: a browser
// without push, a refused permission, or a push service that fails
// (headless browsers) leave the app working without notifications.
import { client } from "./state";

function keyBytes(base64url: string): Uint8Array<ArrayBuffer> {
  const padded = base64url.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (base64url.length % 4)) % 4);
  const raw = atob(padded);
  const out = new Uint8Array(new ArrayBuffer(raw.length));
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}

function encode(buffer: ArrayBuffer | null): string {
  if (!buffer) return "";
  return btoa(String.fromCharCode(...new Uint8Array(buffer))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** Registers the service worker; resolves to the registration or null. */
export async function registerServiceWorker(): Promise<ServiceWorkerRegistration | null> {
  if (typeof navigator === "undefined" || !("serviceWorker" in navigator)) return null;
  try {
    return await navigator.serviceWorker.register("/sw.js", { scope: "/" });
  } catch (e) {
    console.warn("service worker:", e);
    return null;
  }
}

/** Subscribes this browser to Web Push and registers it with the instance. */
export async function enablePush(vapidPublicKey: string): Promise<"subscribed" | "unavailable" | "denied" | "failed"> {
  if (!vapidPublicKey) return "unavailable";
  const registration = await registerServiceWorker();
  if (!registration || !("pushManager" in registration) || typeof Notification === "undefined") return "unavailable";
  if (Notification.permission === "denied") return "denied";
  if (Notification.permission !== "granted" && (await Notification.requestPermission()) !== "granted") return "denied";
  try {
    const existing = await registration.pushManager.getSubscription();
    const subscription =
      existing ??
      (await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: keyBytes(vapidPublicKey) }));
    await client().pushSubscribe(subscription.endpoint, encode(subscription.getKey("p256dh")), encode(subscription.getKey("auth")));
    return "subscribed";
  } catch (e) {
    console.warn("push:", e);
    return "failed";
  }
}
