//! Message bodies through a windowless `rune::Runtime`: what each construct of the
//! protocol's subset draws, what stays as typed, where links go, and selection across
//! messages with what a copy of it holds.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::*;
use lumia_shell::app::State;
use lumia_shell::session::{Entry, Pending};
use rune::runtime::ElementInfo;
use rune::{Key, KeyEvent, Mods, PhysicalInput, Runtime};

const MINUTE: i64 = 60_000_000;

/// `bodies` as the messages `m0`, `m1`, … a minute apart, from Grace and Ada in turn.
fn entries(bodies: &[&str]) -> Vec<Entry> {
    let now = chrono::Utc::now().timestamp_micros();
    bodies
        .iter()
        .enumerate()
        .map(|(i, body)| {
            let author = if i.is_multiple_of(2) { "grace" } else { "me" };
            let at = now - ((bodies.len() - i) as i64) * MINUTE;
            message(&format!("m{i}"), author, body, at)
        })
        .collect()
}

/// A signed-in window whose open channel holds `bodies`.
fn conversation(bodies: &[&str]) -> Runtime<State> {
    let mut state = signed_in();
    state.session.entries = entries(bodies);
    state.session.revision += 1;
    runtime(state)
}

/// The line a copy puts above message `i`'s body: its author, a tab, its time.
fn head(rt: &Runtime<State>, i: usize) -> String {
    let entry = &rt.state.session.entries[i];
    let at = chrono::DateTime::from_timestamp_micros(entry.at_micros)
        .unwrap()
        .with_timezone(&chrono::Local);
    let name = if i.is_multiple_of(2) {
        "Grace Hopper"
    } else {
        "Ada Lovelace"
    };
    format!("{name}\t{}", at.format("%-I:%M %p"))
}

/// The element whose path ends in `suffix` (`message-m0/body/b0`), skipping unkeyed elements
/// between the keys as `Runtime::find` does.
fn element(rt: &Runtime<State>, suffix: &str) -> ElementInfo {
    let keyed = |path: &str| {
        let keys: Vec<&str> = path
            .split('/')
            .filter(|part| !part.starts_with('#'))
            .collect();
        keys.join("/")
    };
    let wanted = format!("/{suffix}");
    rt.elements()
        .into_iter()
        .find(|e| keyed(&e.path).ends_with(&wanted))
        .unwrap_or_else(|| panic!("{suffix} is not on screen"))
}

/// The texts drawn inside the element at `suffix`, in tree order, without the spacing and
/// direction marks drawn around inline code.
fn texts(rt: &Runtime<State>, suffix: &str) -> Vec<String> {
    let root = element(rt, suffix).path;
    let inside = format!("{root}/");
    rt.elements()
        .into_iter()
        .filter(|e| e.path == root || e.path.starts_with(&inside))
        .filter(|e| e.role == "text")
        .filter_map(|e| e.text)
        .map(|text| text.replace(['\u{202f}', '\u{2066}', '\u{2069}'], ""))
        .collect()
}

/// The roles inside the element at `suffix`, in tree order.
fn roles(rt: &Runtime<State>, suffix: &str) -> Vec<&'static str> {
    let inside = format!("{}/", element(rt, suffix).path);
    rt.elements()
        .into_iter()
        .filter(|e| e.path.starts_with(&inside))
        .map(|e| e.role)
        .collect()
}

/// Centres of the glyphs drawn inside the text at `suffix`, in reading order by line.
/// Spaces draw no glyph.
fn glyphs(rt: &Runtime<State>, suffix: &str) -> Vec<(f32, f32)> {
    let e = element(rt, suffix);
    let scale = 2.0;
    let mut out: Vec<(f32, f32)> = rt
        .instances
        .iter()
        .filter(|i| i.params[1] == rune::render::KIND_GLYPH)
        .map(|i| {
            (
                (i.rect[0] + i.rect[2] / 2.0) / scale,
                (i.rect[1] + i.rect[3] / 2.0) / scale,
            )
        })
        .filter(|&(x, y)| {
            x >= e.rect[0]
                && x <= e.rect[0] + e.rect[2]
                && y >= e.rect[1]
                && y <= e.rect[1] + e.rect[3]
        })
        .collect();
    // Lines by ink centre: a new line when the centre jumps by more than a third of one.
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut line = (0, f32::NEG_INFINITY);
    let mut lines: Vec<(usize, (f32, f32))> = out
        .into_iter()
        .map(|g| {
            if g.1 - line.1 > 8.0 {
                line = (line.0 + 1, g.1);
            }
            (line.0, g)
        })
        .collect();
    lines.sort_by(|a, b| (a.0, a.1.0).partial_cmp(&(b.0, b.1.0)).unwrap());
    lines.into_iter().map(|l| l.1).collect()
}

/// Just left of a glyph's centre, and just right of it: nearer its own edge than the next
/// glyph's, however narrow either is.
fn before(g: (f32, f32)) -> (f32, f32) {
    (g.0 - 2.0, g.1)
}
fn after(g: (f32, f32)) -> (f32, f32) {
    (g.0 + 2.0, g.1)
}

/// The start of the text at `suffix`: just inside its left edge, on its first line. A
/// press left of the text is a press on no text.
fn start(rt: &Runtime<State>, suffix: &str) -> (f32, f32) {
    (element(rt, suffix).rect[0] + 0.5, glyphs(rt, suffix)[0].1)
}

/// `got == expected`, saying where they part instead of printing both in full.
#[track_caller]
fn assert_copied(got: &str, expected: &str) {
    if got == expected {
        return;
    }
    let at = got
        .char_indices()
        .zip(expected.chars())
        .find(|((_, a), b)| a != b)
        .map_or(got.len().min(expected.len()), |((i, _), _)| i);
    let from = (0..=at.saturating_sub(40))
        .rev()
        .find(|i| got.is_char_boundary(*i))
        .unwrap_or(0);
    let window = |s: &str| s[from..].chars().take(100).collect::<String>();
    panic!(
        "the copy differs at byte {at} ({} against {} bytes):\n     got …{:?}\nexpected …{:?}",
        got.len(),
        expected.len(),
        window(got),
        window(expected)
    );
}

fn tick(rt: &mut Runtime<State>) {
    rt.frame(1.0 / 60.0);
}

/// Drags from `from` to `to` and returns what a copy would then hold. A second passes
/// first, so a press where the last one landed is not a double-click.
fn drag(rt: &mut Runtime<State>, from: (f32, f32), to: (f32, f32)) -> String {
    rt.frame(1.0);
    rt.pointer_move(from.0, from.1);
    rt.pointer_down(from.0, from.1);
    tick(rt);
    for step in 1..=8 {
        let t = step as f32 / 8.0;
        rt.pointer_move(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        tick(rt);
    }
    rt.pointer_up(to.0, to.1);
    tick(rt);
    rt.selected_text().unwrap_or_default()
}

fn click(rt: &mut Runtime<State>, at: (f32, f32)) {
    rt.frame(1.0);
    rt.pointer_move(at.0, at.1);
    rt.pointer_down(at.0, at.1);
    rt.pointer_up(at.0, at.1);
    tick(rt);
}

fn command(rt: &mut Runtime<State>, c: char) {
    rt.key(KeyEvent {
        key: Key::Command(c),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    tick(rt);
}

#[test]
fn every_construct_of_the_subset_draws_its_text() {
    let rt = conversation(&[
        "A paragraph with *emphasis*, **strong**, ~~strike~~ and inline `code`.\n\nA second,\non two lines.",
        "```rust\nfn main() {\n    println!(\"hi\");\n}\n```",
        "> A quote.\n>\n> Its second paragraph.\n\nAnd the reply.",
        "- one\n- two\n  1. nested\n  2. numbers\n- three",
        "5. five\n6. six",
        "A [link](https://example.com/a) and https://example.com/b.",
    ]);
    assert_eq!(
        texts(&rt, "message-m0/body"),
        [
            "A paragraph with emphasis, strong, strike and inline code.",
            "A second,\non two lines.",
        ]
    );
    // Inline code is drawn between narrow spaces that make room for its chip; assistive
    // technology is given the words without them.
    let para = element(&rt, "message-m0/body/b0");
    let drawn = para.text.unwrap();
    assert!(
        drawn.contains("inline \u{202f}\u{202f}\u{2066}code\u{2069}\u{202f}\u{202f}."),
        "{drawn:?}"
    );
    assert_eq!(
        para.label.as_deref(),
        Some("A paragraph with emphasis, strong, strike and inline code.")
    );

    assert_eq!(
        texts(&rt, "message-m1/body"),
        ["fn main() {\n    println!(\"hi\");\n}"]
    );
    assert_eq!(roles(&rt, "message-m1/body"), ["code", "text"]);

    assert_eq!(
        texts(&rt, "message-m2/body"),
        ["A quote.", "Its second paragraph.", "And the reply."]
    );
    assert_eq!(element(&rt, "message-m2/body/b0").role, "blockquote");

    // A list's markers are drawn, and are not the item's text.
    assert_eq!(
        texts(&rt, "message-m3/body"),
        [
            "•", "one", "•", "two", "1.", "nested", "2.", "numbers", "•", "three"
        ]
    );
    assert_eq!(element(&rt, "message-m3/body/b0").role, "list");
    assert_eq!(element(&rt, "message-m3/body/b0/i1").role, "listitem");
    assert_eq!(element(&rt, "message-m3/body/b0/i1/b1").role, "list");
    assert_eq!(texts(&rt, "message-m4/body"), ["5.", "five", "6.", "six"]);

    assert_eq!(
        texts(&rt, "message-m5/body"),
        ["A link and https://example.com/b."]
    );
    assert!(
        rt.find("https://example.com/a").is_some() && rt.find("https://example.com/b").is_some()
    );
}

#[test]
fn what_is_outside_the_subset_is_drawn_as_it_was_typed() {
    let typed = [
        "# Not a heading",
        "<b>not bold</b> <script>alert(1)</script>",
        "![not an image](https://example.com/cat.png)",
        "| not | a table |\n|-----|---------|\n| 1 | 2 |",
        "[not a web link](file:///etc/passwd)",
        "- [ ] not a checkbox",
    ];
    let mut rt = conversation(&typed);
    for (i, source) in typed.iter().enumerate().take(5) {
        assert_eq!(texts(&rt, &format!("message-m{i}/body")), [*source]);
        assert_eq!(
            roles(&rt, &format!("message-m{i}/body")),
            ["text"],
            "{source}"
        );
    }
    assert_eq!(texts(&rt, "message-m5/body"), ["•", "[ ] not a checkbox"]);
    // Nothing in the list is anything but text, a list and what frames a row.
    let rows = roles(&rt, "rows");
    for role in [
        "heading", "image", "img", "table", "row", "cell", "checkbox", "link",
    ] {
        assert!(!rows.contains(&role), "a {role} was drawn");
    }
    // And nothing asked for the image, or anything else.
    assert!(rt.take_url_requests().is_empty());
}

#[test]
fn every_script_reaches_the_screen_as_it_was_sent() {
    let sent = [
        "日本語のメッセージです。句読点は、どうなりますか。",
        "这是一条中文消息，用来检查换行和标点。",
        "한국어 메시지입니다. 줄바꿈과 문장 부호를 확인합니다.",
        "هذه رسالة باللغة العربية للتحقق من الاتجاه.",
        "Сообщение на русском языке, чтобы проверить кириллицу.",
        "Emoji with joiners: 👍🏽 👨‍👩‍👧‍👦 🏳️‍🌈 🧑🏻‍💻",
    ];
    let mut rt = conversation(&sent);
    for (i, source) in sent.iter().enumerate() {
        assert_eq!(texts(&rt, &format!("message-m{i}/body")), [*source]);
    }
    // Arabic starts at the right edge of its row: the text is laid out right to left.
    let arabic = glyphs(&rt, "message-m3/body/b0");
    let latin = glyphs(&rt, "message-m4/body/b0");
    let row = element(&rt, "message-m3/body/b0").rect;
    let right = arabic.iter().map(|g| g.0).fold(0.0, f32::max);
    assert!(
        row[0] + row[2] - right < 12.0,
        "Arabic ends {right} in a row {row:?}"
    );
    assert!(latin[0].0 - row[0] < 12.0, "Cyrillic starts at the left");
    // A copy of all of it is what was sent.
    click(&mut rt, before(latin[3]));
    command(&mut rt, 'a');
    let copied = rt.selected_text().unwrap();
    for source in sent {
        assert!(copied.contains(source), "{source} is not in {copied:?}");
    }
}

#[test]
fn a_word_wider_than_the_row_breaks_inside_itself() {
    let long = format!("https://example.com/{}", "long/".repeat(60));
    let rt = conversation(&[&long, &format!("```\n{}\n```", "x".repeat(400))]);
    for body in ["message-m0/body/b0", "message-m1/body/b0/code"] {
        let text = element(&rt, body);
        let row = element(&rt, &body[.."message-m0".len()]);
        assert!(
            text.rect[3] > 60.0,
            "{body} takes several lines: {:?}",
            text.rect
        );
        let right = glyphs(&rt, body).iter().map(|g| g.0).fold(0.0, f32::max);
        assert!(
            right < row.rect[0] + row.rect[2],
            "{body} stays inside its row"
        );
    }
}

#[test]
fn a_drag_selects_within_one_message() {
    let mut rt = conversation(&[
        "Shipped on **Friday**, with `code` in it.",
        "Reading it now.",
    ]);
    let g = glyphs(&rt, "message-m0/body/b0");
    // "Shipped on Friday": 7 + 2 + 6 glyphs, spaces drawing none.
    assert_eq!(
        drag(&mut rt, before(g[0]), after(g[14])),
        "Shipped on Friday"
    );
    // Through the inline code, whose padding is not copied.
    let end = *g.last().unwrap();
    assert_eq!(
        drag(&mut rt, before(g[9]), after(end)),
        "Friday, with code in it."
    );
    assert_eq!(element(&rt, "message-m1/body/b0").selected, None);
}

#[test]
fn a_drag_runs_from_one_message_into_the_next_and_across_three() {
    let mut rt = conversation(&[
        "First message here.",
        "Second message here.",
        "Third message here.",
    ]);
    let (a, b, c) = (
        glyphs(&rt, "message-m0/body/b0"),
        glyphs(&rt, "message-m1/body/b0"),
        glyphs(&rt, "message-m2/body/b0"),
    );
    // From "message" in the first to the end of "Second" in the next: the author and time
    // between them are part of it, after a blank line.
    let two = drag(&mut rt, before(a[5]), after(b[5]));
    assert_copied(&two, &format!("message here.\n\n{}\nSecond", head(&rt, 1)));

    let three = drag(&mut rt, before(a[5]), after(c[4]));
    assert_copied(
        &three,
        &format!(
            "message here.\n\n{}\nSecond message here.\n\n{}\nThird",
            head(&rt, 1),
            head(&rt, 2)
        ),
    );
    // Upwards is the same selection.
    assert_copied(&drag(&mut rt, after(c[4]), before(a[5])), &three);
    // From a name, the copy starts with it.
    let name = start(&rt, "message-m1/author");
    let from_name = drag(&mut rt, name, after(c[4]));
    assert_copied(
        &from_name,
        &format!(
            "{}\nSecond message here.\n\n{}\nThird",
            head(&rt, 1),
            head(&rt, 2)
        ),
    );
}

#[test]
fn a_copy_names_a_block_once() {
    let mut state = signed_in();
    let now = chrono::Utc::now().timestamp_micros();
    state.session.entries = vec![
        message("m0", "grace", "First of the block.", now - 2 * MINUTE),
        message("m1", "grace", "Second of the block.", now - MINUTE),
        message("m2", "me", "An answer.", now),
    ];
    state.session.revision += 1;
    let mut rt = runtime(state);
    let last = glyphs(&rt, "message-m2/body/b0");
    let from = start(&rt, "message-m0/author");
    let all = drag(&mut rt, from, after(last[last.len() - 1]));
    // `head` names its even messages Grace's; the third here is Ada's.
    let answer = head(&rt, 2).replace("Grace Hopper", "Ada Lovelace");
    assert_copied(
        &all,
        &format!(
            "{}\nFirst of the block.\n\nSecond of the block.\n\n{answer}\nAn answer.",
            head(&rt, 0),
        ),
    );
}

#[test]
fn a_selection_stays_while_messages_arrive() {
    let mut rt = conversation(&["First message here.", "Second message here."]);
    let (a, b) = (
        glyphs(&rt, "message-m0/body/b0"),
        glyphs(&rt, "message-m1/body/b0"),
    );
    let selected = drag(&mut rt, before(a[5]), after(b[5]));
    assert_copied(
        &selected,
        &format!("message here.\n\n{}\nSecond", head(&rt, 1)),
    );
    rt.update(|s, tx| {
        let at = chrono::Utc::now().timestamp_micros();
        s.session
            .entries
            .push(message("m2", "grace", "A third arrives.", at));
        s.session.revision += 1;
        tx.scroll_to_bottom("messages");
    });
    settle(&mut rt);
    assert!(rt.find("m2").is_some());
    assert_copied(&rt.selected_text().unwrap_or_default(), &selected);
}

#[test]
fn a_drag_through_a_list_a_quote_and_a_code_block_copies_them_as_text() {
    let mut rt = conversation(&[
        "Before the list:\n\n- one\n- two\n  1. nested\n  2. numbers\n- three\n\n\
         > A quote,\n> on two lines.\n>\n> - with an item\n\n```rust\nfn main() {\n\tlet x = 1;\n}\n```\n\nAnd after.",
        "The next message.",
    ]);
    let next = glyphs(&rt, "message-m1/body/b0");
    let first = start(&rt, "message-m0/body/b0");
    let copied = drag(&mut rt, first, after(next[2]));
    assert_copied(
        &copied,
        &format!(
            "Before the list:\n\n\
             - one\n\
             - two\n  1. nested\n  2. numbers\n\
             - three\n\n\
             > A quote,\n> on two lines.\n\n\
             > - with an item\n\n\
             fn main() {{\n\tlet x = 1;\n}}\n\n\
             And after.\n\n\
             {}\nThe",
            head(&rt, 1)
        ),
    );
    // Starting inside a list item: that item goes without its marker, the next keep theirs.
    let two = glyphs(&rt, "message-m0/body/b1/i1/b0");
    let three = glyphs(&rt, "message-m0/body/b1/i2/b0");
    assert_copied(
        &drag(&mut rt, before(two[1]), after(three[4])),
        "wo\n  1. nested\n  2. numbers\n- three",
    );
    // One item alone is its text.
    let item = start(&rt, "message-m0/body/b1/i2/b0");
    assert_copied(&drag(&mut rt, item, after(three[4])), "three");
}

#[test]
fn command_a_takes_the_conversation_and_command_c_copies_it_as_a_transcript() {
    let mut rt = conversation(&[
        "First.\n\nIts second paragraph.",
        "- a list\n- of two",
        "Last.",
    ]);
    rt.update(|s, _| {
        s.session.entries[2].deleted = true;
        s.session.pending.push(Pending {
            id: "p1".into(),
            channel_id: "general".into(),
            body: "Not stored yet.".into(),
            sent_at_micros: 0,
            failed: None,
            reply: None,
        });
        s.session.revision += 1;
    });
    settle(&mut rt);
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);

    let g = glyphs(&rt, "message-m1/body/b0/i0/b0");
    click(&mut rt, before(g[2]));
    command(&mut rt, 'a');
    let transcript = format!(
        "{}\nFirst.\n\nIts second paragraph.\n\n{}\n- a list\n- of two\n\n{}\n(deleted)",
        head(&rt, 0),
        head(&rt, 1),
        head(&rt, 2)
    );
    assert_copied(&rt.selected_text().unwrap(), &transcript);
    // The send that is still pending is on screen and in no selection.
    assert!(words(&rt).contains("Not stored yet."));

    command(&mut rt, 'c');
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some(transcript.as_str())
    );
}

/// One line draws each message's time and name before its words, and a copy still reads
/// as grouped: a block's author and time once, on a line over its first body.
#[test]
fn one_line_copies_as_grouped_does() {
    let mut state = signed_in();
    let now = chrono::Utc::now().timestamp_micros();
    state.session.entries = vec![
        message("m0", "grace", "First of the block.", now - 3 * MINUTE),
        message("m1", "grace", "- a list\n- of two", now - 2 * MINUTE),
        message("m2", "me", "An answer.", now - MINUTE),
        message("m3", "grace", "Gone.", now),
    ];
    state.session.entries[3].deleted = true;
    state.settings.apply(lumia_shell::settings::Preset::Minimal);
    state.session.revision += 1;
    let mut rt = runtime(state);
    // The time and the name are drawn on each line, the block's second included.
    assert!(rt.find("message-m1/time").is_some());
    let answer = head(&rt, 2).replace("Grace Hopper", "Ada Lovelace");
    let transcript = format!(
        "{}\nFirst of the block.\n\n- a list\n- of two\n\n{answer}\nAn answer.\n\n{}\n(deleted)",
        head(&rt, 0),
        head(&rt, 3).replace("Ada Lovelace", "Grace Hopper"),
    );
    let g = glyphs(&rt, "message-m0/body/b0");
    click(&mut rt, before(g[2]));
    command(&mut rt, 'a');
    assert_copied(&rt.selected_text().unwrap(), &transcript);
    // A drag within a block takes no head; one into the next block takes that block's.
    let first = glyphs(&rt, "message-m0/body/b0");
    let item = glyphs(&rt, "message-m1/body/b0/i1/b0");
    let answered = glyphs(&rt, "message-m2/body/b0");
    let (from, to) = (
        before(first[5]),
        after(glyphs(&rt, "message-m1/body/b0/i0/b0")[2]),
    );
    assert_copied(&drag(&mut rt, from, to), "of the block.\n\n- a li");
    let (from, to) = (before(item[0]), after(answered[1]));
    assert_copied(
        &drag(&mut rt, from, to),
        &format!("- of two\n\n{answer}\nAn"),
    );
    // A selection inside one body takes neither.
    let (from, to) = (before(answered[0]), after(answered[1]));
    assert_copied(&drag(&mut rt, from, to), "An");
}

#[test]
fn a_selection_reaches_rows_scrolled_out_of_the_mounted_window() {
    let bodies: Vec<String> = (0..400).map(|i| format!("Message number {i}.")).collect();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let mut rt = conversation(&refs);
    assert!(
        rt.find("m399").is_some() && rt.find("m0").is_none(),
        "the list opens at its end, the start unmounted"
    );

    // Everything: 400 messages, all but a screenful never built.
    let newest = glyphs(&rt, "message-m399/body/b0");
    click(&mut rt, before(newest[3]));
    command(&mut rt, 'a');
    let all = rt.selected_text().unwrap();
    let expected: Vec<String> = (0..400)
        .map(|i| format!("{}\nMessage number {i}.", head(&rt, i)))
        .collect();
    assert_copied(&all, &expected.join("\n\n"));

    // A press in the newest message, a long scroll up, and a shift-click there: the
    // selection runs between them through the rows that are mounted no more.
    click(&mut rt, after(newest[6]));
    let (x, y) = (newest[0].0, 400.0);
    rt.pointer_move(x, y);
    for _ in 0..40 {
        rt.wheel(x, y, 300.0);
        tick(&mut rt);
    }
    settle(&mut rt);
    assert!(
        rt.find("m399").is_none(),
        "the newest message scrolled out and unmounted"
    );
    let top = (0..400)
        .find(|i| {
            rt.find(&format!("message-m{i}/body/b0"))
                .is_some_and(|(_, y)| y > 150.0 && y < 600.0)
        })
        .expect("a message is on screen");
    let old = glyphs(&rt, &format!("message-m{top}/body/b0"));
    rt.physical_input(PhysicalInput::Modifiers(Mods {
        shift: true,
        ..Mods::default()
    }));
    click(&mut rt, before(old[7]));
    rt.physical_input(PhysicalInput::Modifiers(Mods::default()));
    let span = rt.selected_text().unwrap();
    let mut expected = vec![format!("number {top}.")];
    expected.extend((top + 1..399).map(|i| format!("{}\nMessage number {i}.", head(&rt, i))));
    expected.push(format!("{}\nMessage", head(&rt, 399)));
    assert_copied(&span, &expected.join("\n\n"));
}

#[test]
fn links_open_only_as_web_addresses() {
    let mut rt = conversation(&[
        "[the plan](https://example.com/plan) and http://example.com/bare",
        "[a file](file:///etc/passwd) [a script](javascript:alert(1)) [mail](mailto:ada@example.com) <lumia://join/x>",
    ]);
    rt.update(|s, _| {
        s.session.pending.push(Pending {
            id: "p1".into(),
            channel_id: "general".into(),
            body: "[pending](https://example.com/pending)".into(),
            sent_at_micros: 0,
            failed: None,
            reply: None,
        });
        s.session.revision += 1;
    });
    settle(&mut rt);

    for url in ["https://example.com/plan", "http://example.com/bare"] {
        let at = rt
            .find(url)
            .unwrap_or_else(|| panic!("{url} is not a link"));
        click(&mut rt, at);
        assert_eq!(rt.take_url_requests(), [url]);
    }
    // The others are not links at all: their text is what was typed, and a press on any
    // part of it opens nothing.
    assert_eq!(
        texts(&rt, "message-m1/body"),
        [
            "[a file](file:///etc/passwd) [a script](javascript:alert(1)) [mail](mailto:ada@example.com) <lumia://join/x>"
        ]
    );
    for url in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "mailto:ada@example.com",
        "lumia://join/x",
    ] {
        assert!(rt.find(url).is_none(), "{url} is a link");
    }
    for g in glyphs(&rt, "message-m1/body/b0") {
        click(&mut rt, g);
    }
    assert!(rt.take_url_requests().is_empty());
    // A send that is not stored yet shows its link and does not open it.
    assert_eq!(texts(&rt, "message-p1/body"), ["pending"]);
    for g in glyphs(&rt, "message-p1/body/b0") {
        click(&mut rt, g);
    }
    assert!(rt.take_url_requests().is_empty());
}
