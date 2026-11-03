// The key the OPFS stores are encrypted under at rest (e2ee spec section
// 8): a random 32-byte database key, wrapped by a non-extractable AES-GCM
// WebCrypto key generated once per origin and kept in IndexedDB beside
// the wrapped bytes. The database key reaches SQLite as raw bytes in the
// worker; a copy of the OPFS files is useless without the browser
// profile that holds the wrapping key.
const DB = "lumia-store-key";
const STORE = "keys";
const ID = "origin";

interface KeyRecord {
  id: string;
  key: CryptoKey;
  wrapped: ArrayBuffer;
  iv: Uint8Array<ArrayBuffer>;
}

function randomBytes(n: number): Uint8Array<ArrayBuffer> {
  return crypto.getRandomValues(new Uint8Array(new ArrayBuffer(n)));
}

function fail(req: IDBRequest): Error {
  return req.error ?? new Error("indexeddb");
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: "id" });
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(fail(req));
  });
}

function read(db: IDBDatabase): Promise<KeyRecord | undefined> {
  return new Promise((resolve, reject) => {
    const req = db.transaction(STORE, "readonly").objectStore(STORE).get(ID) as IDBRequest<KeyRecord | undefined>;
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(fail(req));
  });
}

/**
 * Stores `candidate` unless a record exists, in one transaction, and
 * resolves to whichever record is stored: two first uses at once (two
 * opens in flight) must end with one key, or one store would be written
 * under a key that is then overwritten.
 */
function claim(db: IDBDatabase, candidate: KeyRecord): Promise<KeyRecord> {
  return new Promise((resolve, reject) => {
    const store = db.transaction(STORE, "readwrite").objectStore(STORE);
    const get = store.get(ID) as IDBRequest<KeyRecord | undefined>;
    get.onerror = () => reject(fail(get));
    get.onsuccess = () => {
      if (get.result) {
        resolve(get.result);
        return;
      }
      const put = store.put(candidate);
      put.onsuccess = () => resolve(candidate);
      put.onerror = () => reject(fail(put));
    };
  });
}

async function unwrap(record: KeyRecord): Promise<Uint8Array> {
  const plain = await crypto.subtle.decrypt({ name: "AES-GCM", iv: record.iv }, record.key, record.wrapped);
  return new Uint8Array(plain);
}

/** The raw database key for this origin, created on first use. */
export async function storeKey(): Promise<Uint8Array> {
  const db = await openDb();
  try {
    const found = await read(db);
    if (found) return await unwrap(found);
    // First use: the wrapping key and the database key are made before
    // the transaction, since WebCrypto awaits would end it.
    const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
    const raw = randomBytes(32);
    const iv = randomBytes(12);
    const wrapped = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, raw);
    const candidate: KeyRecord = { id: ID, key, wrapped, iv };
    const stored = await claim(db, candidate);
    return stored === candidate ? raw : await unwrap(stored);
  } finally {
    db.close();
  }
}
