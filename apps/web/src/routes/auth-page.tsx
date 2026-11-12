// The page the sign-in board draws, shared by every screen outside the
// app: the stacked logo over a card in the middle, a line under the
// card, and the instance's own links at the foot.
import { Show, children, type JSX } from "solid-js";

export interface AuthPageProps {
  /** The stacked logo over the card; left out where the card stands alone. */
  logo?: boolean;
  /** A line under the card: the way to the other screen. */
  under?: JSX.Element;
  /** Links at the page's foot. */
  footer?: JSX.Element;
  children?: JSX.Element;
}

export function AuthPage(props: AuthPageProps) {
  // Resolved once each: a `when` that read the prop would build its elements a second time.
  const under = children(() => props.under);
  const footer = children(() => props.footer);
  return (
    <div class="auth">
      <main class="auth__main">
        <div class="auth__column">
          <Show when={props.logo ?? true}>
            <div class="auth__logo" aria-hidden="true">
              <img src="/brand/lumia-original-light.svg" alt="" class="lu-only-light" />
              <img src="/brand/lumia-original-dark.svg" alt="" class="lu-only-dark" />
            </div>
          </Show>
          {props.children}
          <Show when={under()}>
            <p class="auth__under">{under()}</p>
          </Show>
        </div>
      </main>
      <Show when={footer()}>
        <footer class="auth__footer">{footer()}</footer>
      </Show>
    </div>
  );
}

/** The instance a screen is about: its host and who runs it (architecture 6.1), as an inset block. */
export function TrustCard(props: { host: string; operatorName: string; softwareVersion?: string; onChange?: () => void }) {
  return (
    <div class="auth__trust">
      <span class="auth__trust-text">
        <span class="auth__trust-host">{props.host}</span>
        <span class="auth__trust-op">
          run by {props.operatorName || "an unnamed operator"}
          {props.softwareVersion ? ` · Lumia ${props.softwareVersion}` : ""}
        </span>
      </span>
      <Show when={props.onChange}>
        <button type="button" class="lu-link auth__change" onClick={() => props.onChange?.()}>
          Change instance
        </button>
      </Show>
    </div>
  );
}
