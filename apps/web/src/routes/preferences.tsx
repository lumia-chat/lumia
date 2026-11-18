// Preferences: how Lumia looks in this browser, the "Preferences · Light"
// and "· Dark" boards. Appearance as three tiles; Messages as three preset
// cards over outlined groups of compact choices and switches; beside them
// (under Messages where the page is narrow) a preview of messages drawn by
// the conversation's own rows under the settings as they stand. Each
// choice takes effect as it is made (`client/preferences.ts`). The
// desktop's scroll bar choice is not here: a page cannot keep it.
import { For, Show, type JSX } from "solid-js";
import {
  Attachment,
  ChannelHeader,
  LayoutSketch,
  LinkCard,
  Message,
  MessageBody,
  MessageList,
  PictureChoices,
  Segmented,
  SettingRow,
  SettingsGroup,
  ThemeSketch,
  Toggle,
  type MessagePart,
  type ReactionChipProps,
} from "../design/components";
import type { BodyBlock } from "../client/bridge";
import { sizeWords } from "../client/media";
import {
  FONT_SIZES,
  choosePreset,
  preferences,
  presetOf,
  setAppearance,
  setPreferences,
  switchCaption,
  switchOn,
  switchValue,
  type Appearance,
  type Display,
  type Preferences,
  type Preset,
  type SwitchSetting,
} from "../client/preferences";
import { PREVIEW_CHANNEL, PREVIEW_MESSAGES, previewAt, previewDrawn, type PreviewMessage } from "../client/preview";
import { byBlock, initialsOf, shortTimeOf, timeOf } from "./app";

const SWITCHES: { setting: SwitchSetting; name: string }[] = [
  { setting: "images", name: "Images" },
  { setting: "embeds", name: "Embeds" },
  { setting: "reactions", name: "Reactions" },
];

function Head(props: { id: string; title: string; subtitle: string }) {
  return (
    <div class="lu-prefs__head">
      <h2 id={props.id} class="lu-prefs__title">
        {props.title}
      </h2>
      <p class="lu-prefs__subtitle">{props.subtitle}</p>
    </div>
  );
}

/** The page in the app: this browser's preferences, each choice kept as it is made. */
export function PreferencesPage(props: { back: () => void }) {
  return <PreferencesView value={preferences()} onAppearance={setAppearance} onPreset={choosePreset} onChange={setPreferences} back={props.back} />;
}

export interface PreferencesViewProps {
  value: Preferences;
  onAppearance: (appearance: Appearance) => void;
  /** Writes the preset's values. */
  onPreset: (preset: Preset) => void;
  onChange: (next: Partial<Preferences>) => void;
  back?: () => void;
}

/** The page drawn from the values it is given (the gallery's board hands it its own). */
export function PreferencesView(props: PreferencesViewProps) {
  const appearanceId = "lu-prefs-appearance";
  const messagesId = "lu-prefs-messages";
  const previewId = "lu-prefs-preview";
  const value = () => props.value;
  const preset = () => presetOf(value());
  const set = (next: Partial<Preferences>) => props.onChange(next);
  return (
    <>
      <ChannelHeader kind="page" name="Preferences" topic="How Lumia looks in this browser" actions={[]} onBack={props.back} backLabel="Back to Home" />
      <div class="lu-page">
        <div class="lu-prefs">
          <div class="lu-prefs__grid">
            <section aria-labelledby={appearanceId} class="lu-prefs__section" style={{ "grid-area": "appearance" }}>
              <Head id={appearanceId} title="Appearance" subtitle="Light, dark, or whichever this device is in." />
              <PictureChoices
                labelledBy={appearanceId}
                value={value().appearance}
                onChange={(appearance) => props.onAppearance(appearance)}
                options={[
                  { value: "light", label: "Light", picture: <ThemeSketch theme="light" /> },
                  { value: "dark", label: "Dark", picture: <ThemeSketch theme="dark" /> },
                  { value: "system", label: "System", picture: <ThemeSketch theme="system" /> },
                ]}
              />
            </section>
            {/* A preset writes its values; the card lit is the one they match, or none and "Custom". */}
            <section aria-labelledby={messagesId} class="lu-prefs__section" style={{ "grid-area": "messages" }}>
              <Head id={messagesId} title="Messages" subtitle="Start from a preset, then change anything below." />
              <PictureChoices
                label="Preset"
                value={preset()}
                onChange={(preset) => props.onPreset(preset)}
                options={[
                  { value: "comfortable", label: "Comfortable", caption: "Large avatars, room to breathe", picture: <LayoutSketch layout="comfortable" /> },
                  { value: "compact", label: "Compact", caption: "Small avatars, more on screen", picture: <LayoutSketch layout="compact" /> },
                  { value: "minimal", label: "Minimal", caption: "A line each, media as links", picture: <LayoutSketch layout="minimal" /> },
                ]}
              />
              <Show when={!preset()}>
                <p role="status" class="lu-prefs__custom">
                  Custom: these settings match no preset.
                </p>
              </Show>
              <SettingsGroup>
                <SettingRow
                  name="Layout"
                  caption="One person’s messages as a block, or a line each."
                  control={(ids) => (
                    <Segmented
                      labelledBy={ids.name}
                      options={[
                        { value: "grouped", label: "Grouped" },
                        { value: "one_line", label: "One line" },
                      ]}
                      value={value().layout}
                      onChange={(layout) => set({ layout })}
                    />
                  )}
                />
                <SettingRow
                  name="Spacing"
                  caption="Room between messages."
                  control={(ids) => (
                    <Segmented
                      labelledBy={ids.name}
                      options={[
                        { value: "roomy", label: "Roomy" },
                        { value: "cozy", label: "Cozy" },
                        { value: "tight", label: "Tight" },
                      ]}
                      value={value().spacing}
                      onChange={(spacing) => set({ spacing })}
                    />
                  )}
                />
                {/* One line draws no avatar, so it offers none. */}
                <Show when={value().layout === "grouped"}>
                  <SettingRow
                    name="Avatars"
                    caption="Beside each block of messages."
                    control={(ids) => (
                      <Segmented
                        labelledBy={ids.name}
                        options={[
                          { value: "large", label: "Large" },
                          { value: "small", label: "Small" },
                          { value: "hidden", label: "Hidden" },
                        ]}
                        value={value().avatars}
                        onChange={(avatars) => set({ avatars })}
                      />
                    )}
                  />
                </Show>
                <SettingRow
                  name="Font size"
                  caption="Messages and names, in points."
                  control={(ids) => (
                    <>
                      <span aria-hidden="true" class="lu-prefs__a">
                        A
                      </span>
                      <Segmented
                        even
                        labelledBy={ids.name}
                        options={FONT_SIZES.map((size) => ({ value: size, label: String(size), name: `${size} points` }))}
                        value={value().fontSize}
                        onChange={(fontSize) => set({ fontSize })}
                      />
                      <span aria-hidden="true" class="lu-prefs__a lu-prefs__a--large">
                        A
                      </span>
                    </>
                  )}
                />
              </SettingsGroup>
              <SettingsGroup>
                <For each={SWITCHES}>
                  {(s) => (
                    <SettingRow
                      name={s.name}
                      caption={switchCaption(value(), s.setting)}
                      control={(ids) => <Toggle labelledBy={ids.name} describedBy={ids.caption} checked={switchOn(value(), s.setting)} onChange={(on) => set(switchValue(s.setting, on))} />}
                    />
                  )}
                </For>
              </SettingsGroup>
            </section>
            <aside aria-labelledby={previewId} class="lu-prefs__preview">
              <h2 id={previewId} class="lu-prefs__label">
                Preview
              </h2>
              <MessagesPreview display={value()} />
              <p class="lu-prefs__kept">Kept on this browser only. Your other devices and your account are not changed.</p>
            </aside>
          </div>
        </div>
      </div>
    </>
  );
}

/** A sample message's body as the core would read it: one paragraph, its link marked. */
function blocksOf(m: PreviewMessage): BodyBlock[] {
  const at = m.link ? m.text.indexOf(m.link.words) : -1;
  const links = m.link && at >= 0 ? [{ start: at, end: at + m.link.words.length, url: m.link.url }] : [];
  return [{ kind: "para", text: m.text, runs: [], links }];
}

/**
 * What a sample message carries under its body, in the place and frame
 * `MessageMedia` (routes/media.tsx) draws it, from the same components
 * where they can be drawn from sample data: the file row is `Attachment`
 * and the card `LinkCard`, as there. `MessageMedia` itself is not used:
 * it fetches each tile's thumbnail and asks the core for a card's embed.
 * The picture is drawn here, in the tile's own frame (`lu-media__tile`),
 * since a tile shows only a fetched thumbnail and the sample has no file.
 */
function PreviewMedia(props: { m: PreviewMessage; display: Display }) {
  const drawn = () => previewDrawn(props.m, props.display);
  return (
    <Show when={drawn().picture || drawn().fileRow || drawn().card}>
      <div class="lu-mediablock">
        <Show when={drawn().picture}>
          <div class="lu-media">
            <figure class="lu-media__tile lu-media__tile--image">
              <span class="lu-preview__shot">
                <span class="lu-preview__screen">
                  <span class="lu-preview__title" />
                  <span class="lu-preview__field" />
                  <span class="lu-preview__field" />
                  <span class="lu-preview__button" />
                </span>
              </span>
            </figure>
          </div>
        </Show>
        <Show when={drawn().fileRow ? props.m.picture : undefined}>{(pic) => <Attachment kind="image" name={pic().name} size={sizeWords(pic().bytes)} onOpen={() => {}} onDownload={() => {}} />}</Show>
        <Show when={drawn().card ? props.m.card : undefined}>{(c) => <LinkCard url={c().url} host={c().host} title={c().title} description={c().description} />}</Show>
      </div>
    </Show>
  );
}

/**
 * The preview: the sample messages in blocks as a conversation makes them
 * (`byBlock`), each drawn by the conversation's own rows (`MessageList`,
 * `Message`, `MessageBody`, the reaction chips) with the layout, spacing,
 * avatars, font size, images, embeds and reactions given. One picture to
 * assistive technology, and nothing in it can be pressed or focused.
 */
export function MessagesPreview(props: { display: Display }) {
  const d = () => props.display;
  const blocks = byBlock(PREVIEW_MESSAGES, (m) => m.authorId, previewAt);
  const chips = (m: PreviewMessage): ReactionChipProps[] => previewDrawn(m, d()).reactions.map((r) => ({ emoji: r.emoji, label: r.emoji, count: r.count, selected: r.mine }));
  const body = (m: PreviewMessage): JSX.Element => (
    <>
      <MessageBody blocks={blocksOf(m)} />
      <PreviewMedia m={m} display={d()} />
    </>
  );
  return (
    <div role="img" aria-label="Preview of messages with these settings" class="lu-preview">
      <div class="lu-preview__head">
        <span class="lu-preview__hash">#</span>
        <span class="lu-preview__name">{PREVIEW_CHANNEL}</span>
      </div>
      <div inert class="lu-preview__body">
        <MessageList label="Preview" layout={d().layout} spacing={d().spacing} avatars={d().avatars}>
          <For each={blocks}>
            {(block) => {
              const first = block.first;
              const t = timeOf(previewAt(first));
              const parts = block.rest.map(
                (m): MessagePart => ({
                  // As the timeline says it: whole in one line; grouped, in the avatar's column, without its "AM" or "PM".
                  get time() {
                    return d().layout === "one_line" ? timeOf(previewAt(m)).time : shortTimeOf(previewAt(m));
                  },
                  datetime: timeOf(previewAt(m)).datetime,
                  children: body(m),
                  id: m.id,
                  get reactions() {
                    return chips(m);
                  },
                }),
              );
              return (
                <Message
                  author={first.author}
                  initials={initialsOf(first.author)}
                  tone={first.tone}
                  time={t.time}
                  datetime={t.datetime}
                  actions={false}
                  layout={d().layout}
                  avatars={d().avatars}
                  id={first.id}
                  reactions={chips(first)}
                  parts={parts}
                >
                  {body(first)}
                </Message>
              );
            }}
          </For>
        </MessageList>
      </div>
    </div>
  );
}
