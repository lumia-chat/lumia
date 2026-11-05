// Passkeys on the page side (architecture 6.3): the instance makes the
// options and checks the answer; this is the browser's own ceremony in
// between, with the JSON forms the WebAuthn Level 3 API speaks.
import { client } from "./state";

type Credential = PublicKeyCredential & { toJSON(): unknown };

/** Whether this browser can make and use passkeys. */
export function passkeysAvailable(): boolean {
  return typeof PublicKeyCredential !== "undefined" && "parseCreationOptionsFromJSON" in PublicKeyCredential;
}

/** Makes a passkey for the signed-in account and registers it with the instance. */
export async function addPasskey(label: string): Promise<void> {
  const start = await client().passkeyRegisterStart();
  const options = PublicKeyCredential.parseCreationOptionsFromJSON(JSON.parse(start.optionsJson) as PublicKeyCredentialCreationOptionsJSON);
  const credential = (await navigator.credentials.create({ publicKey: options })) as Credential | null;
  if (!credential) throw new Error("no passkey was made");
  await client().passkeyRegisterFinish(start.challengeId, JSON.stringify(credential.toJSON()), label);
}

/** Signs in with a passkey, for a handle when given (a discoverable passkey needs none). */
export async function signInWithPasskey(handleOrEmail: string): Promise<void> {
  const start = await client().passkeyAssertStart(handleOrEmail);
  const options = PublicKeyCredential.parseRequestOptionsFromJSON(JSON.parse(start.optionsJson) as PublicKeyCredentialRequestOptionsJSON);
  const credential = (await navigator.credentials.get({ publicKey: options })) as Credential | null;
  if (!credential) throw new Error("no passkey was offered");
  await client().signInPasskey(start.challengeId, JSON.stringify(credential.toJSON()));
}
