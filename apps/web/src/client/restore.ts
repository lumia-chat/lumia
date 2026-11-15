// Restoring from the recovery phrase (identity spec 4.3): the rules the
// restore page follows that need no page. It asks first whether a device
// is still at hand (then linking is what is wanted), and once the phrase
// is in, asks the core which devices restoring would stop: none, and it
// restores at once; some, and the person ticks the ones they still have.
import type { DeviceSummary } from "./bridge";

/** What follows the phrase: restoring at once, or "Keep any devices?" first. */
export function afterPhrase(devices: DeviceSummary[]): "restore" | "keep" {
  return devices.some((d) => !d.revoked) ? "keep" : "restore";
}

/** The button under the devices, by how many are ticked. */
export function keepWords(count: number): string {
  if (count === 0) return "Restore and stop them all";
  return `Restore and keep ${count} ${count === 1 ? "device" : "devices"}`;
}

/** A device's name: its label, or its kind's when it has none. */
export function deviceName(d: Pick<DeviceSummary, "kind" | "label">): string {
  if (d.label.trim()) return d.label.trim();
  return d.kind === "web" ? "Browser" : d.kind === "native" ? "Computer" : "Device";
}

/** The line under its name: "Added Nov 12", a web one's "On chat.acme.dev · added Nov 1". */
export function addedLine(d: Pick<DeviceSummary, "kind" | "webOrigin" | "addedAtMicros">, locale?: string): string {
  const date = d.addedAtMicros ? new Date(d.addedAtMicros / 1000).toLocaleDateString(locale, { month: "short", day: "numeric" }) : "";
  const host = d.kind === "web" && d.webOrigin ? hostOf(d.webOrigin) : "";
  if (host) return date ? `On ${host} · added ${date}` : `On ${host}`;
  return date ? `Added ${date}` : "";
}

function hostOf(origin: string): string {
  try {
    return new URL(origin).host || origin;
  } catch {
    return origin;
  }
}
