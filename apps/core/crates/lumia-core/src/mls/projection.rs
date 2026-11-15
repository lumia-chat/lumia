//! The local projection of a conversation (spec section 5): a deterministic
//! function of its operations in `seq` order.

use std::collections::{BTreeMap, BTreeSet};

use crate::proto::{dm_operation, Attachment, Card, DmOperation};

/// One message as the projection shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmMessage {
    /// The message id from the `send`.
    pub message_id: Vec<u8>,
    /// Sequence number of the `send` in the conversation's log.
    pub seq: u64,
    /// The sending identity.
    pub sender_identity_id: Vec<u8>,
    /// The sending device.
    pub sender_device_id: Vec<u8>,
    /// Current body; empty when deleted.
    pub body: String,
    /// Thread parent, or empty.
    pub parent_id: Vec<u8>,
    /// Attachments as sent, less those an edit dropped.
    pub attachments: Vec<Attachment>,
    /// Link cards the sender's client built, only those under a link the
    /// text holds (specs/media.md 6.5).
    pub cards: Vec<Card>,
    /// Mentioned identity ids.
    pub mentions: Vec<Vec<u8>>,
    /// The sender's `sent_at`, nanoseconds since the epoch (display only).
    pub sent_at_nanos: i128,
    /// Whether an edit has been applied.
    pub edited: bool,
    /// Whether a delete has tombstoned it.
    pub deleted: bool,
    /// Reactions: emoji to the identities that set it.
    pub reactions: BTreeMap<String, BTreeSet<Vec<u8>>>,
    /// What it quotes (architecture 6.6), emptied when that is deleted.
    pub quote: Option<DmQuoted>,
}

/// A quote in a personal conversation: the message quoted, by id, and the
/// text quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmQuoted {
    /// The message quoted.
    pub message_id: Vec<u8>,
    /// What of it was quoted; empty once it is deleted.
    pub text: String,
    /// The message quoted was deleted, and the text with it.
    pub original_deleted: bool,
}

/// The projection of one conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Projection {
    messages: BTreeMap<Vec<u8>, DmMessage>,
    order: Vec<Vec<u8>>,
    /// Pinned message ids.
    pub pins: BTreeSet<Vec<u8>>,
    /// Each identity's read mark (`through_seq`), newest wins.
    pub read: BTreeMap<Vec<u8>, u64>,
    /// Typing identities with their `until`, nanoseconds; never persisted.
    pub typing: BTreeMap<Vec<u8>, i128>,
}

impl Projection {
    /// Applies one operation at `seq` from `sender`, per the table in spec
    /// section 5. Unknown or malformed operations are ignored.
    pub fn apply(
        &mut self,
        seq: u64,
        sender_identity: &[u8],
        sender_device: &[u8],
        op: &DmOperation,
    ) {
        let Some(inner) = &op.op else { return };
        match inner {
            dm_operation::Op::Send(send) => {
                if send.message_id.is_empty() || self.messages.contains_key(&send.message_id) {
                    return;
                }
                self.order.push(send.message_id.clone());
                self.messages.insert(
                    send.message_id.clone(),
                    DmMessage {
                        message_id: send.message_id.clone(),
                        seq,
                        sender_identity_id: sender_identity.to_vec(),
                        sender_device_id: sender_device.to_vec(),
                        body: send.body.clone(),
                        parent_id: send.parent_id.clone(),
                        attachments: send
                            .attachments
                            .iter()
                            .take(crate::media::MAX_ATTACHMENTS)
                            .cloned()
                            .collect(),
                        cards: cards_in(&send.body, &send.cards),
                        mentions: send.mentions.clone(),
                        sent_at_nanos: crate::identity::keys::ts_nanos(op.sent_at.as_ref()),
                        edited: false,
                        deleted: false,
                        reactions: BTreeMap::new(),
                        quote: send.quote.as_ref().map(|q| DmQuoted {
                            message_id: q.message_id.clone(),
                            text: q.text.clone(),
                            original_deleted: false,
                        }),
                    },
                );
            }
            dm_operation::Op::Edit(edit) => {
                if let Some(m) = self.messages.get_mut(&edit.message_id) {
                    if m.sender_identity_id == sender_identity && !m.deleted {
                        m.body.clone_from(&edit.body);
                        m.edited = true;
                        m.attachments
                            .retain(|a| !edit.remove_attachment_ids.contains(&a.attachment_id));
                        // Set, the cards are these from now on; unset, those
                        // whose links the new text still holds stay.
                        let cards = edit.cards.as_ref().map_or(&m.cards, |c| &c.cards);
                        m.cards = cards_in(&m.body, cards);
                    }
                }
            }
            dm_operation::Op::Delete(delete) => {
                let deleted = match self.messages.get_mut(&delete.message_id) {
                    Some(m) if m.sender_identity_id == sender_identity => {
                        m.body.clear();
                        m.deleted = true;
                        m.attachments.clear();
                        m.cards.clear();
                        self.pins.remove(&delete.message_id);
                        true
                    }
                    _ => false,
                };
                // Deleting still deletes where the message was quoted.
                if deleted {
                    for reply in self.messages.values_mut() {
                        if let Some(q) = reply
                            .quote
                            .as_mut()
                            .filter(|q| q.message_id == delete.message_id)
                        {
                            q.text.clear();
                            q.original_deleted = true;
                        }
                    }
                }
            }
            dm_operation::Op::React(react) => {
                if let Some(m) = self.messages.get_mut(&react.message_id) {
                    let who = m.reactions.entry(react.emoji.clone()).or_default();
                    if react.remove {
                        who.remove(sender_identity);
                        if who.is_empty() {
                            m.reactions.remove(&react.emoji);
                        }
                    } else {
                        who.insert(sender_identity.to_vec());
                    }
                }
            }
            dm_operation::Op::Pin(pin) => {
                if self.messages.contains_key(&pin.message_id) {
                    if pin.unpin {
                        self.pins.remove(&pin.message_id);
                    } else {
                        self.pins.insert(pin.message_id.clone());
                    }
                }
            }
            dm_operation::Op::Read(read) => {
                let mark = self.read.entry(sender_identity.to_vec()).or_insert(0);
                *mark = (*mark).max(read.through_seq);
            }
            dm_operation::Op::Typing(typing) => {
                self.typing.insert(
                    sender_identity.to_vec(),
                    crate::identity::keys::ts_nanos(typing.until.as_ref()),
                );
            }
        }
    }

    /// Messages in send order.
    pub fn messages(&self) -> impl Iterator<Item = &DmMessage> {
        self.order.iter().filter_map(|id| self.messages.get(id))
    }

    /// One message by id.
    #[must_use]
    pub fn message(&self, id: &[u8]) -> Option<&DmMessage> {
        self.messages.get(id)
    }
}

/// The cards of `cards` a receiver draws under `body` (specs/media.md 6.5):
/// only one whose URL is a link the text holds ([`crate::cards::card_links`]),
/// the first for each, at most [`crate::cards::MAX_CARDS`]. A forged card
/// can mislead about a page, never about where its link goes.
#[must_use]
pub fn cards_in(body: &str, cards: &[Card]) -> Vec<Card> {
    let links = crate::cards::card_links(body);
    let mut kept: Vec<Card> = Vec::new();
    for card in cards {
        if kept.len() >= crate::cards::MAX_CARDS {
            break;
        }
        if links.contains(&card.url) && !kept.iter().any(|k| k.url == card.url) {
            kept.push(card.clone());
        }
    }
    kept
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::proto::{DmDelete, DmEdit, DmPin, DmReact, DmRead, DmSend};

    fn send(id: u8, body: &str) -> DmOperation {
        DmOperation {
            operation_id: vec![id; 16],
            sent_at: None,
            op: Some(dm_operation::Op::Send(DmSend {
                message_id: vec![id; 16],
                body: body.into(),
                ..Default::default()
            })),
        }
    }

    fn op(inner: dm_operation::Op) -> DmOperation {
        DmOperation {
            operation_id: vec![],
            sent_at: None,
            op: Some(inner),
        }
    }

    #[test]
    fn a_quote_loses_its_text_when_the_original_is_deleted() {
        use crate::proto::DmQuote;
        let a = vec![1; 32];
        let dev = vec![9; 16];
        let mut p = Projection::default();
        p.apply(1, &a, &dev, &send(1, "regrettable"));
        let mut reply = send(2, "ha");
        if let Some(dm_operation::Op::Send(s)) = reply.op.as_mut() {
            s.quote = Some(DmQuote {
                message_id: vec![1; 16],
                text: "regrettable".into(),
            });
        }
        p.apply(2, &a, &dev, &reply);
        assert_eq!(
            p.message(&[2; 16]).unwrap().quote.as_ref().unwrap().text,
            "regrettable"
        );
        p.apply(
            3,
            &a,
            &dev,
            &op(dm_operation::Op::Delete(DmDelete {
                message_id: vec![1; 16],
            })),
        );
        let quote = p.message(&[2; 16]).unwrap().quote.clone().unwrap();
        assert!(quote.text.is_empty() && quote.original_deleted);
    }

    #[test]
    fn sends_edits_deletes_and_reactions_follow_the_table() {
        let a = vec![1; 32];
        let b = vec![2; 32];
        let dev = vec![9; 16];
        let mut p = Projection::default();
        p.apply(1, &a, &dev, &send(1, "hi"));
        p.apply(2, &a, &dev, &send(1, "dup ignored"));
        assert_eq!(p.messages().count(), 1);
        assert_eq!(p.message(&[1; 16]).unwrap().body, "hi");

        // Only the author edits.
        p.apply(
            3,
            &b,
            &dev,
            &op(dm_operation::Op::Edit(DmEdit {
                message_id: vec![1; 16],
                body: "hacked".into(),
                ..Default::default()
            })),
        );
        assert_eq!(p.message(&[1; 16]).unwrap().body, "hi");
        p.apply(
            4,
            &a,
            &dev,
            &op(dm_operation::Op::Edit(DmEdit {
                message_id: vec![1; 16],
                body: "hello".into(),
                ..Default::default()
            })),
        );
        let m = p.message(&[1; 16]).unwrap();
        assert_eq!(m.body, "hello");
        assert!(m.edited);

        // Anyone reacts; a remove clears their own.
        let react = |remove| {
            op(dm_operation::Op::React(DmReact {
                message_id: vec![1; 16],
                emoji: "👍".into(),
                remove,
            }))
        };
        p.apply(5, &b, &dev, &react(false));
        p.apply(6, &a, &dev, &react(false));
        assert_eq!(p.message(&[1; 16]).unwrap().reactions["👍"].len(), 2);
        p.apply(7, &b, &dev, &react(true));
        assert_eq!(p.message(&[1; 16]).unwrap().reactions["👍"].len(), 1);

        // Any member pins; the newest read mark wins; only the author deletes.
        p.apply(
            8,
            &b,
            &dev,
            &op(dm_operation::Op::Pin(DmPin {
                message_id: vec![1; 16],
                unpin: false,
            })),
        );
        assert!(p.pins.contains(&vec![1; 16]));
        p.apply(
            9,
            &b,
            &dev,
            &op(dm_operation::Op::Read(DmRead { through_seq: 8 })),
        );
        p.apply(
            10,
            &b,
            &dev,
            &op(dm_operation::Op::Read(DmRead { through_seq: 3 })),
        );
        assert_eq!(p.read[&b], 8);
        p.apply(
            11,
            &b,
            &dev,
            &op(dm_operation::Op::Delete(DmDelete {
                message_id: vec![1; 16],
            })),
        );
        assert!(!p.message(&[1; 16]).unwrap().deleted);
        p.apply(
            12,
            &a,
            &dev,
            &op(dm_operation::Op::Delete(DmDelete {
                message_id: vec![1; 16],
            })),
        );
        let m = p.message(&[1; 16]).unwrap();
        assert!(m.deleted && m.body.is_empty());
        assert!(!p.pins.contains(&vec![1; 16]));
    }

    #[test]
    fn attachments_and_cards_follow_sends_edits_and_deletes() {
        use crate::proto::{DmCards, DmSend};
        let a = vec![1; 32];
        let dev = vec![9; 16];
        let card = |url: &str| Card {
            url: url.into(),
            title: "A page".into(),
            ..Default::default()
        };
        let attachment = |id: u8| Attachment {
            attachment_id: vec![id; 16],
            name: format!("{id}.jpg"),
            ..Default::default()
        };
        let mut p = Projection::default();
        p.apply(
            1,
            &a,
            &dev,
            &DmOperation {
                operation_id: vec![1; 16],
                sent_at: None,
                op: Some(dm_operation::Op::Send(DmSend {
                    message_id: vec![1; 16],
                    body: "see https://a.example and `https://b.example`".into(),
                    attachments: vec![attachment(1), attachment(2)],
                    // A card for a link in code, and one for no link at all,
                    // are forgeries a receiver does not draw.
                    cards: vec![
                        card("https://a.example"),
                        card("https://b.example"),
                        card("https://evil.example"),
                        card("https://a.example"),
                    ],
                    ..Default::default()
                })),
            },
        );
        let m = p.message(&[1; 16]).unwrap();
        assert_eq!(m.attachments.len(), 2);
        let urls: Vec<_> = m.cards.iter().map(|c| c.url.as_str()).collect();
        assert_eq!(urls, ["https://a.example"]);

        // An edit drops an attachment, and its new text keeps the card.
        p.apply(
            2,
            &a,
            &dev,
            &op(dm_operation::Op::Edit(DmEdit {
                message_id: vec![1; 16],
                body: "still https://a.example, also https://c.example".into(),
                remove_attachment_ids: vec![vec![1; 16]],
                cards: None,
            })),
        );
        let m = p.message(&[1; 16]).unwrap();
        assert_eq!(m.attachments, vec![attachment(2)]);
        assert_eq!(m.cards.len(), 1);

        // Set, the cards are replaced; a link gone from the text loses its card.
        p.apply(
            3,
            &a,
            &dev,
            &op(dm_operation::Op::Edit(DmEdit {
                message_id: vec![1; 16],
                body: "now https://c.example".into(),
                remove_attachment_ids: vec![],
                cards: Some(DmCards {
                    cards: vec![card("https://a.example"), card("https://c.example")],
                }),
            })),
        );
        let urls: Vec<_> = p
            .message(&[1; 16])
            .unwrap()
            .cards
            .iter()
            .map(|c| c.url.clone())
            .collect();
        assert_eq!(urls, ["https://c.example"]);

        p.apply(
            4,
            &a,
            &dev,
            &op(dm_operation::Op::Delete(DmDelete {
                message_id: vec![1; 16],
            })),
        );
        let m = p.message(&[1; 16]).unwrap();
        assert!(m.attachments.is_empty() && m.cards.is_empty());
    }
}
