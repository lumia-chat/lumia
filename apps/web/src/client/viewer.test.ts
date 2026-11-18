import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { VIEW_FIT, detailScale, held, maxScale, ownSize, panBy, pressed, viewFrame, wheelFactor, zoomAt, zoomStep, type View } from "./viewer";

interface Picture {
  own: [number, number];
  room: [number, number];
  margin: number;
}
type Move =
  | { kind: "zoom_at"; scale: number; at: [number, number] }
  | { kind: "pan_by"; by: [number, number] }
  | { kind: "step"; steps: number }
  | { kind: "pressed"; at: [number, number] }
  | { kind: "own_size" }
  | { kind: "held" };

// The cases every client agrees on (lumia_core::viewer).
const cases = JSON.parse(readFileSync(resolve(import.meta.dir, "../../../../proto/vectors/viewer/cases.json"), "utf8")) as {
  frames: (Picture & { width: number; height: number; own_scale: number; max_scale: number; detail_scale: number; why: string })[];
  moves: { picture: Picture; view: View; move: Move; gives: View; why: string }[];
  wheel: { delta: number; pinch: boolean; factor: number; why: string }[];
};

const frameOf = (p: Picture) => viewFrame(p.own[0], p.own[1], p.room[0], p.room[1], p.margin);
// To a thousandth, as the cases say.
const near = (a: number, b: number) => Math.abs(a - b) < 1e-3;

describe("the shared cases", () => {
  test("a picture is fitted, and how far it zooms", () => {
    expect(cases.frames.length).toBeGreaterThan(0);
    for (const c of cases.frames) {
      const f = frameOf(c);
      const same = near(f.width, c.width) && near(f.height, c.height) && near(f.own, c.own_scale) && near(maxScale(f), c.max_scale) && near(detailScale(f), c.detail_scale);
      expect({ why: c.why, same, frame: same ? undefined : f }).toEqual({ why: c.why, same: true, frame: undefined });
    }
  });

  test("what each move makes of a view", () => {
    expect(cases.moves.length).toBeGreaterThan(0);
    for (const c of cases.moves) {
      const f = frameOf(c.picture);
      const m = c.move;
      const got =
        m.kind === "zoom_at"
          ? zoomAt(c.view, m.scale, { x: m.at[0], y: m.at[1] }, f)
          : m.kind === "pan_by"
            ? panBy(c.view, m.by[0], m.by[1], f)
            : m.kind === "step"
              ? zoomStep(c.view, m.steps, f)
              : m.kind === "pressed"
                ? pressed(c.view, { x: m.at[0], y: m.at[1] }, f)
                : m.kind === "own_size"
                  ? ownSize(c.view, f)
                  : held(c.view, f);
      const same = near(got.scale, c.gives.scale) && near(got.x, c.gives.x) && near(got.y, c.gives.y);
      expect({ why: c.why, same, got: same ? undefined : got }).toEqual({ why: c.why, same: true, got: undefined });
    }
  });

  test("what a wheel event zooms by", () => {
    expect(cases.wheel.length).toBeGreaterThan(0);
    for (const c of cases.wheel) expect({ why: c.why, same: near(wheelFactor(c.delta, c.pinch), c.factor) }).toEqual({ why: c.why, same: true });
  });
});

describe("the viewer's zoom", () => {
  const frame = viewFrame(2000, 1000, 1000, 800);
  test("the spot under the pointer stays under it", () => {
    const v = zoomAt(VIEW_FIT, 2, { x: 100, y: 50 }, frame);
    expect(v).toEqual({ scale: 2, x: -100, y: -50 });
    // The picture's point 100 right of its centre was under the pointer; at twice the size it is 200 from the centre, moved back by 100.
    expect(100 * v.scale + v.x).toBe(100);
  });
  test("a zoomed picture pressed is fitted again, exactly", () => {
    expect(pressed({ scale: 2, x: -100, y: -50 }, { x: 7, y: 7 }, frame)).toBe(VIEW_FIT);
    expect(zoomAt({ scale: 2, x: -100, y: -50 }, 0.2, { x: 0, y: 0 }, frame)).toEqual(VIEW_FIT);
  });
});
