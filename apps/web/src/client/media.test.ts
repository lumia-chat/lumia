import { describe, expect, test } from "bun:test";
import { BlobUrls, attachErrorWords, debounce, editCardUrls, fileKind, fitSize, keptCardUrls, mediaGrid, panBy, pasteTakesFiles, sizeWords, sniffImage, tooLargeWords, usageWords, zoomAt } from "./media";

/** Timers run by hand. */
function fakeTimers() {
  let now = 0;
  let next = 1;
  const due = new Map<number, { at: number; f: () => void }>();
  return {
    set: (f: () => void, ms: number) => {
      const id = next++;
      due.set(id, { at: now + ms, f });
      return id;
    },
    clear: (h: unknown) => void due.delete(h as number),
    advance(ms: number) {
      now += ms;
      for (const [id, t] of [...due].sort((a, b) => a[1].at - b[1].at)) {
        if (t.at > now) continue;
        due.delete(id);
        t.f();
      }
    },
    pending: () => due.size,
  };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("fitSize", () => {
  test("a tall picture is drawn 400 tall at its shape", () => {
    expect(fitSize(3000, 4000)).toEqual({ width: 300, height: 400 });
  });
  test("a small one is never drawn larger", () => {
    expect(fitSize(120, 80)).toEqual({ width: 120, height: 80 });
  });
  test("a wide one is held to the width given", () => {
    expect(fitSize(4000, 1000, 400, 500)).toEqual({ width: 500, height: 125 });
  });
  test("unknown dimensions are a 4:3 box", () => {
    expect(fitSize(0, 0)).toEqual({ width: 320, height: 240 });
  });
});

describe("mediaGrid", () => {
  const rows = (n: number) => mediaGrid(n).map((c) => 6 / c.span);
  test("one is alone", () => {
    expect(mediaGrid(1)).toEqual([{ span: 6, height: 400 }]);
    expect(mediaGrid(0)).toEqual([]);
  });
  test("rows of three, with rows of two first where three do not divide", () => {
    expect(rows(2)).toEqual([2, 2]);
    expect(rows(3)).toEqual([3, 3, 3]);
    expect(rows(4)).toEqual([2, 2, 2, 2]);
    expect(rows(5)).toEqual([2, 2, 3, 3, 3]);
    expect(rows(7)).toEqual([2, 2, 2, 2, 3, 3, 3]);
    expect(rows(10)).toEqual([2, 2, 2, 2, 3, 3, 3, 3, 3, 3]);
  });
  test("every row fills the six columns", () => {
    for (let n = 2; n <= 10; n++) {
      const cells = mediaGrid(n);
      expect(cells.length).toBe(n);
      expect(cells.reduce((s, c) => s + c.span, 0) % 6).toBe(0);
    }
  });
});

describe("sizeWords", () => {
  test("bytes, KB and MB of 1,024", () => {
    expect(sizeWords(1)).toBe("1 byte");
    expect(sizeWords(512)).toBe("512 bytes");
    expect(sizeWords(2048)).toBe("2 KB");
    expect(sizeWords(1536)).toBe("1.5 KB");
    expect(sizeWords(48 * 1024)).toBe("48 KB");
    expect(sizeWords(10 * 1024 * 1024)).toBe("10 MB");
    expect(sizeWords(2.4 * 1024 * 1024)).toBe("2.4 MB");
  });
});

describe("files", () => {
  test("a kind by type, else by name", () => {
    expect(fileKind("IMG_1.HEIC", "")).toBe("heic");
    expect(fileKind("a.heic", "image/heic")).toBe("heic");
    expect(fileKind("clip.mov", "video/quicktime")).toBe("video");
    expect(fileKind("clip.mov", "")).toBe("video");
    expect(fileKind("song.flac", "")).toBe("audio");
    expect(fileKind("photo.jpg", "image/jpeg")).toBe("still");
    expect(fileKind("anim.gif", "image/gif")).toBe("other");
    expect(fileKind("report.pdf", "application/pdf")).toBe("other");
  });
  test("over the file cap is refused before reading, except a still picture", () => {
    const cap = 10 * 1024 * 1024;
    expect(tooLargeWords(cap + 1, "other", cap)).toBe("Too large: files can be up to 10 MB here.");
    expect(tooLargeWords(cap + 1, "video", cap)).toBeDefined();
    expect(tooLargeWords(cap + 1, "still", cap)).toBeUndefined();
    expect(tooLargeWords(cap, "other", cap)).toBeUndefined();
  });
  test("the core's refusals in words", () => {
    expect(attachErrorWords("this file is 99 bytes, over the limit of 10")).toBe("Too large to send.");
    expect(attachErrorWords("this file is 99 bytes, over the limit of 10", "", 10 * 1024 * 1024)).toBe("Too large: files can be up to 10 MB here.");
    expect(attachErrorWords("this instance does not keep attachments")).toBe("This instance doesn’t keep attachments.");
    expect(attachErrorWords("this picture is too large to send, even made smaller")).toBe("This picture is too large to send, even made smaller.");
    expect(attachErrorWords("nope", "PERMISSION_DENIED")).toBe("You can’t attach files here.");
    expect(attachErrorWords("quota exceeded", "RESOURCE_EXHAUSTED", 10 * 1024 * 1024, "storage")).toBe("This server is out of space for files.");
    expect(attachErrorWords("something odd details=reason=x")).toBe("Something odd.");
  });
  test("a paste is files when it holds no words but their names", () => {
    expect(pasteTakesFiles("", ["image.png"])).toBe(true);
    expect(pasteTakesFiles("report.pdf", ["report.pdf"])).toBe(true);
    expect(pasteTakesFiles("a.png\nb.png", ["a.png", "b.png"])).toBe(true);
    expect(pasteTakesFiles("Some words from a document", ["image.png"])).toBe(false);
    expect(pasteTakesFiles("words", [])).toBe(false);
  });
  test("a picture's type from its bytes", () => {
    expect(sniffImage(new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d]))).toBe("image/png");
    expect(sniffImage(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe("image/jpeg");
    expect(sniffImage(new TextEncoder().encode("GIF89a"))).toBe("image/gif");
    expect(sniffImage(new TextEncoder().encode("RIFF....WEBPVP8 "))).toBe("image/webp");
    expect(sniffImage(new Uint8Array([1, 2, 3]))).toBeUndefined();
  });
});

describe("cards", () => {
  test("a composer keeps the links not put away, at most three", () => {
    const links = ["https://a.example", "https://b.example", "https://c.example", "https://d.example"];
    expect(keptCardUrls(links, new Set())).toEqual(links.slice(0, 3));
    expect(keptCardUrls(links, new Set(["https://b.example"]))).toEqual(["https://a.example", "https://c.example", "https://d.example"]);
  });
  test("an edit leaves the cards alone when nothing about them changed", () => {
    expect(editCardUrls(["https://a.example"], ["https://a.example"], ["https://a.example"], new Set())).toBeNull();
  });
  test("an edit names the cards kept when one is put away", () => {
    expect(editCardUrls(["https://a.example", "https://b.example"], ["https://a.example", "https://b.example"], ["https://a.example", "https://b.example"], new Set(["https://a.example"]))).toEqual(["https://b.example"]);
  });
  test("a link new to the body gets a card; one put away at send does not come back", () => {
    // b's card was put away when it was sent: it is in the old links but not among the cards.
    expect(editCardUrls(["https://a.example"], ["https://a.example", "https://b.example"], ["https://a.example", "https://b.example", "https://c.example"], new Set())).toEqual(["https://a.example", "https://c.example"]);
  });
  test("a link taken out of the body takes its card with it", () => {
    expect(editCardUrls(["https://a.example"], ["https://a.example"], ["https://c.example"], new Set())).toEqual(["https://c.example"]);
  });
});

describe("debounce", () => {
  test("runs once typing pauses, with the last value", () => {
    const t = fakeTimers();
    const ran: string[] = [];
    const d = debounce((v: string) => ran.push(v), 500, t);
    d.call("h");
    t.advance(300);
    d.call("he");
    t.advance(300);
    d.call("hey");
    t.advance(499);
    expect(ran).toEqual([]);
    t.advance(1);
    expect(ran).toEqual(["hey"]);
  });
  test("cancel drops what was waiting", () => {
    const t = fakeTimers();
    const ran: string[] = [];
    const d = debounce((v: string) => ran.push(v), 500, t);
    d.call("x");
    d.cancel();
    t.advance(1000);
    expect(ran).toEqual([]);
  });
});

describe("zoom", () => {
  test("the spot under the pointer stays under it", () => {
    const v = zoomAt({ scale: 1, x: 0, y: 0 }, { x: 100, y: 50 }, 2);
    expect(v).toEqual({ scale: 2, x: -100, y: -50 });
    // The point (100, 50) from the centre was picture point (100, 50); at 2x it is at 2*100 - 100 = 100.
    expect(100 * v.scale + v.x).toBe(100);
  });
  test("held between fit and four times, centred at fit", () => {
    expect(zoomAt({ scale: 2, x: 30, y: 30 }, { x: 0, y: 0 }, 0.5)).toEqual({ scale: 1, x: 0, y: 0 });
    expect(zoomAt({ scale: 1, x: 0, y: 0 }, { x: 0, y: 0 }, 9).scale).toBe(4);
  });
  test("a drag stops where the picture's edge would leave the stage", () => {
    expect(panBy({ scale: 2, x: 0, y: 0 }, 500, -500, { width: 400, height: 300 })).toEqual({ scale: 2, x: 200, y: -150 });
    expect(panBy({ scale: 1, x: 0, y: 0 }, 50, 50, { width: 400, height: 300 })).toEqual({ scale: 1, x: 0, y: 0 });
  });
});

describe("BlobUrls", () => {
  function urls() {
    const t = fakeTimers();
    let made = 0;
    const revoked: string[] = [];
    const cache = new BlobUrls(
      () => `blob:${++made}`,
      (u) => revoked.push(u),
      30_000,
      t,
    );
    return { cache, t, revoked, made: () => made };
  }

  test("one URL per key, however many rows draw it", async () => {
    const { cache, made } = urls();
    let loads = 0;
    const load = async () => {
      loads++;
      return new Blob(["x"]);
    };
    const a = cache.acquire("thumb:1", load);
    const b = cache.acquire("thumb:1", load);
    await flush();
    expect(a.url()).toBe("blob:1");
    expect(b.url()).toBe("blob:1");
    expect(a.state()).toBe("ready");
    expect(loads).toBe(1);
    expect(made()).toBe(1);
  });

  test("a row drawn again within the linger finds the same URL", async () => {
    const { cache, t, revoked } = urls();
    const load = async () => new Blob(["x"]);
    cache.acquire("k", load);
    await flush();
    cache.release("k");
    t.advance(10_000);
    const again = cache.acquire("k", load);
    t.advance(60_000);
    expect(again.url()).toBe("blob:1");
    expect(revoked).toEqual([]);
  });

  test("a URL nobody holds is revoked after the linger", async () => {
    const { cache, t, revoked } = urls();
    cache.acquire("k", async () => new Blob(["x"]));
    await flush();
    cache.release("k");
    t.advance(29_999);
    expect(revoked).toEqual([]);
    t.advance(1);
    expect(revoked).toEqual(["blob:1"]);
    expect(cache.has("k")).toBe(false);
  });

  test("a load that fails or answers nothing is not drawn", async () => {
    const { cache } = urls();
    const failed = cache.acquire("a", async () => {
      throw new Error("NOT_FOUND");
    });
    const out = cache.acquire("b", async () => null);
    await flush();
    expect(failed.state()).toBe("failed");
    expect(out.state()).toBe("failed");
    expect(out.url()).toBeUndefined();
  });

  test("clear revokes everything at once", async () => {
    const { cache, revoked } = urls();
    cache.acquire("a", async () => new Blob(["x"]));
    cache.acquire("b", async () => new Blob(["y"]));
    await flush();
    cache.clear();
    expect(revoked.sort()).toEqual(["blob:1", "blob:2"]);
  });
});

describe("a server's storage", () => {
  test("names the limit only where there is one", () => {
    const gb = 1024 * 1024 * 1024;
    expect(usageWords({ usedBytes: 1.2 * gb, maxBytes: 0 })).toBe("1.2 GB used");
    expect(usageWords({ usedBytes: 1.2 * gb, maxBytes: 5 * gb })).toBe("1.2 GB of 5 GB used");
  });
});
