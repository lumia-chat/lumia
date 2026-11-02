// The key the OPFS stores are encrypted under at rest (e2ee spec section
// 8): a random 32-byte database key, wrapped by a non-extractable AES-GCM
// WebCrypto key generated once per origin and kept in IndexedDB beside
// the wrapped bytes. The database key reaches SQLite as raw bytes in the
// worker; a copy of the OPFS files is useless without the browser
// profile that holds the wrapping key.
const DB = "lumia-store-key";
const STORE = "keys";
const ID = "origin";

interface Record {
  id: string;
  key: CryptoKey;
  wrapped: ArrayBuffer;
  iv: Uint8Array<ArrayBuffer>;
}

function randomBytes(n: number): Uint8Array<ArrayBuffer> {
  return crypto.getRandomValues(new Uint8Array(new ArrayBuffer(n)));
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: "id" });
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error("indexeddb"));
  });
}

function tx<T>(db: IDBDatabase, mode: IDBTransactionMode, run: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    const req = run(db.transaction(STORE, mode).objectStore(STORE));
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error("indexeddb"));
  });
}

/** The raw database key for this origin, created on first use. */
export async function storeKey(): Promise<Uint8Array> {
  const db = await openDb();
  try {
    const found = (await tx<Record | undefined>(db, "readonly", (s) => s.get(ID) as IDBRequest<Record | undefined>)) ?? null;
    if (found) {
      const plain = await crypto.subtle.decrypt({ name: "AES-GCM", iv: found.iv }, found.key, found.wrapped);
      return new Uint8Array(plain);
    }
    const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
    const raw = randomBytes(32);
    const iv = randomBytes(12);
    const wrapped = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, raw);
    await tx(db, "readwrite", (s) => s.put({ id: ID, key, wrapped, iv } satisfies Record));
    return raw;
  } finally {
    db.close();
  }
}
