//! The Roast a Provider writes from a Digest. A reply is untrusted: it is parsed, cut down to size
//! and only then kept, so that a Provider cannot blow up the screen or a Share Site.

use crate::digest::{is_hidden, Digest};
use crate::error::{Error, Result};
use crate::llm::{self, CompletionRequest, Provider};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::time::Duration;

/// Far past any real reply. It is here so nobody parses a flood.
const MAX_REPLY_BYTES: usize = 4 << 20;
const MAX_BURNS: usize = 12;
const MAX_HEADLINE: usize = 160;
const MAX_PARAGRAPH: usize = 1500;
const MAX_SENTENCE: usize = 400;
const MAX_EVIDENCE: usize = 8;
const MAX_GALAXY_NAME: usize = 60;
const MAX_GALAXY_NAMES: usize = 64;

const REMINDER: &str =
    "\n\nYour last reply was not a valid Roast. Reply with only the JSON document \
                        this time, with nothing before or after it.";

/// One titled joke.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Burn {
    pub title: String,
    pub body: String,
}

/// The final judgement. `score` is out of 100.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub archetype: String,
    #[serde(deserialize_with = "clamped_score")]
    pub score: u8,
    pub sentence: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GalaxyName {
    pub id: u32,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Roast {
    pub headline: String,
    pub opening: String,
    pub burns: Vec<Burn>,
    pub verdict: Verdict,
    /// A Provider writes `galaxy_names`, as the contract in the prompt says.
    #[serde(default, alias = "galaxy_names")]
    pub galaxy_names: Vec<GalaxyName>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredRoast {
    pub id: i64,
    pub created_at: i64,
    pub provider_label: String,
    pub roast: Roast,
}

fn clamped_score<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<u8, D::Error> {
    Ok(f64::deserialize(d)?.clamp(0.0, 100.0).round() as u8)
}

pub fn default_system_prompt() -> &'static str {
    include_str!("../../../prompts/roast.md")
}

/// The same contract as the prompt, for a Provider that can enforce one.
fn schema() -> serde_json::Value {
    let text = json!({ "type": "string" });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["headline", "opening", "burns", "verdict", "galaxy_names"],
        "properties": {
            "headline": text,
            "opening": text,
            "burns": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_BURNS,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "body"],
                    "properties": { "title": text, "body": text },
                },
            },
            "verdict": {
                "type": "object",
                "additionalProperties": false,
                "required": ["archetype", "score", "sentence", "evidence"],
                "properties": {
                    "archetype": text,
                    "score": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "sentence": text,
                    "evidence": { "type": "array", "items": text },
                },
            },
            "galaxy_names": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "name"],
                    "properties": { "id": { "type": "integer", "minimum": 0 }, "name": text },
                },
            },
        },
    })
}

fn clean(text: &str, max_chars: usize) -> String {
    let visible: String = text
        .chars()
        .filter(|&c| !is_hidden(c))
        .take(max_chars)
        .collect();
    visible.trim().to_owned()
}

/// `None` when a part the screen cannot do without has nothing in it.
fn validated(roast: Roast) -> Option<Roast> {
    let burns: Vec<Burn> = roast
        .burns
        .iter()
        .map(|b| Burn {
            title: clean(&b.title, MAX_HEADLINE),
            body: clean(&b.body, MAX_PARAGRAPH),
        })
        .filter(|b| !b.title.is_empty() && !b.body.is_empty())
        .take(MAX_BURNS)
        .collect();
    let kept = Roast {
        headline: clean(&roast.headline, MAX_HEADLINE),
        opening: clean(&roast.opening, MAX_PARAGRAPH),
        burns,
        verdict: Verdict {
            archetype: clean(&roast.verdict.archetype, MAX_HEADLINE),
            score: roast.verdict.score,
            sentence: clean(&roast.verdict.sentence, MAX_SENTENCE),
            evidence: roast
                .verdict
                .evidence
                .iter()
                .map(|e| clean(e, MAX_SENTENCE))
                .filter(|e| !e.is_empty())
                .take(MAX_EVIDENCE)
                .collect(),
        },
        galaxy_names: roast
            .galaxy_names
            .iter()
            .map(|g| GalaxyName {
                id: g.id,
                name: clean(&g.name, MAX_GALAXY_NAME),
            })
            .filter(|g| !g.name.is_empty())
            .take(MAX_GALAXY_NAMES)
            .collect(),
    };
    let whole = [
        &kept.headline,
        &kept.opening,
        &kept.verdict.archetype,
        &kept.verdict.sentence,
    ]
    .iter()
    .all(|text| !text.is_empty());
    (whole && !kept.burns.is_empty()).then_some(kept)
}

/// The first JSON object in the reply that is a Roast, which gets past code fences, prose around it
/// and any stray braces in that prose.
fn parse_reply(reply: &str) -> Option<Roast> {
    if reply.len() > MAX_REPLY_BYTES {
        return None;
    }
    let roast = reply.match_indices('{').find_map(|(at, _)| {
        serde_json::Deserializer::from_str(&reply[at..])
            .into_iter::<Roast>()
            .next()?
            .ok()
    })?;
    validated(roast)
}

/// The asking is a parameter so that a test can be the Provider. A Provider that fails gets no second
/// go, because a logged out CLI stays logged out and a timeout has cost long enough already.
fn generate_with(
    mut ask: impl FnMut(&CompletionRequest) -> Result<String>,
    label: &str,
    digest: &Digest,
    system: &str,
) -> Result<Roast> {
    if digest.is_empty() {
        return Err(Error::Provider(
            "There is nothing to roast yet. Add your history on the Sources screen, or try the demo \
             data there."
                .to_owned(),
        ));
    }
    let mut request = CompletionRequest {
        system: system.to_owned(),
        user: format!(
            "<digest>\n{}\n</digest>\n\nWrite the Roast of the person in this Digest. Reply with only \
             the JSON document.",
            serde_json::to_string(digest)?
        ),
        schema: schema(),
    };
    if let Some(roast) = parse_reply(&ask(&request)?) {
        return Ok(roast);
    }
    request.user.push_str(REMINDER);
    parse_reply(&ask(&request)?).ok_or_else(|| {
        Error::Provider(format!(
            "{label} replied twice without a Roast Rostery could use. Your last Roast is unchanged, \
             so try again."
        ))
    })
}

pub fn generate(p: &Provider, digest: &Digest, system: &str, timeout: Duration) -> Result<Roast> {
    generate_with(
        |request| llm::complete(p, request, timeout),
        p.label(),
        digest,
        system,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::SourceDigest;
    use crate::store::Store;
    use serde_json::{json, Value};
    use std::cell::RefCell;

    fn digest() -> Digest {
        Digest {
            today: "2027-01-15".into(),
            spotify: Some(SourceDigest {
                plays: 10,
                minutes: 30,
                minutes_are_estimates: false,
                creators: 2,
                items: 3,
                first_day: "2024-03-01".into(),
                last_day: "2024-03-02".into(),
                skip_rate_percent: 10.0,
                late_night_percent: 20.0,
                deleted_videos: 0,
                biggest_day: None,
                longest_repeat: None,
                top_creators: Vec::new(),
                top_items: Vec::new(),
            }),
            youtube: None,
            galaxies: Vec::new(),
        }
    }

    fn reply() -> Value {
        json!({
            "headline": "Ten Plays, No Witnesses",
            "opening": "You pressed play ten times. Science has questions.",
            "burns": [
                {"title": "The Skip", "body": "10% of the time you changed your mind."},
                {"title": "Night Shift", "body": "20% of it after midnight."},
                {"title": "Variety", "body": "Two Creators. Two."},
            ],
            "verdict": {
                "archetype": "The Cautious Listener",
                "score": 42,
                "sentence": "You have taste, and you are hiding it well.",
                "evidence": ["10% skip rate", "2 Creators"],
            },
            "galaxy_names": [{"id": 0, "name": "The Quiet Room"}],
        })
    }

    /// A fake Provider: gives these replies in order and writes down each request it got.
    fn fake(replies: &[&str]) -> (RefCell<Vec<CompletionRequest>>, Vec<String>) {
        (
            RefCell::new(Vec::new()),
            replies.iter().map(|r| r.to_string()).collect(),
        )
    }

    fn run(replies: &[&str], digest: &Digest) -> (Result<Roast>, Vec<CompletionRequest>) {
        let (seen, answers) = fake(replies);
        let result = generate_with(
            |req| {
                let call = seen.borrow().len();
                seen.borrow_mut().push(req.clone());
                Ok(answers[call].clone())
            },
            "Claude Code",
            digest,
            "Be mean",
        );
        (result, seen.into_inner())
    }

    fn good() -> String {
        reply().to_string()
    }

    #[test]
    fn the_request_holds_the_system_prompt_and_the_digest_in_its_tag() {
        let good = good();
        let (result, seen) = run(&[&good], &digest());
        assert!(result.is_ok());
        assert_eq!(seen[0].system, "Be mean");
        let user = &seen[0].user;
        let json = serde_json::to_string(&digest()).unwrap();
        assert!(
            user.contains(&format!("<digest>\n{json}\n</digest>")),
            "{user}"
        );
    }

    #[test]
    fn extracts_fenced_json() {
        let fenced = format!("```json\n{}\n```", good());
        let (result, seen) = run(&[&fenced], &digest());
        assert_eq!(result.unwrap().headline, "Ten Plays, No Witnesses");
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn extracts_json_after_prose() {
        let chatty = format!(
            "Here is {{your}} Roast, with {{\"a\": 1}} and all:\n\n{}\n\nHope that stings!",
            good()
        );
        let (result, seen) = run(&[&chatty], &digest());
        assert_eq!(result.unwrap().burns.len(), 3);
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn invalid_json_retries_once() {
        let good = good();
        let (result, seen) = run(&["Sorry, I can't do JSON today.", &good], &digest());
        assert!(result.is_ok());
        assert_eq!(seen.len(), 2, "one retry, and no third call");
        assert!(
            seen[1].user.contains("<digest>"),
            "the retry still has the Digest"
        );
        assert!(seen[1].user.contains("only the JSON"), "and a reminder");
        assert!(!seen[0].user.contains("only the JSON document this time"));
    }

    #[test]
    fn two_bad_replies_fail_clearly() {
        let (result, seen) = run(&["nope", "{\"headline\": \"half a"], &digest());
        assert_eq!(seen.len(), 2);
        let message = result.unwrap_err().to_string();
        assert!(message.contains("Claude Code"), "{message}");
        assert!(message.contains("try again"), "{message}");
    }

    #[test]
    fn a_provider_failure_is_not_retried() {
        let calls = RefCell::new(0);
        let result = generate_with(
            |_| {
                *calls.borrow_mut() += 1;
                Err(Error::Provider("Claude Code is not logged in.".into()))
            },
            "Claude Code",
            &digest(),
            "Be mean",
        );
        assert_eq!(*calls.borrow(), 1);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Claude Code is not logged in."
        );
    }

    #[test]
    fn a_provider_failure_on_the_retry_keeps_its_own_message() {
        let calls = RefCell::new(0);
        let result = generate_with(
            |_| {
                *calls.borrow_mut() += 1;
                match *calls.borrow() {
                    1 => Ok("not json".to_owned()),
                    _ => Err(Error::Provider("Claude Code did not finish.".into())),
                }
            },
            "Claude Code",
            &digest(),
            "Be mean",
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "Claude Code did not finish."
        );
    }

    #[test]
    fn score_is_clamped() {
        for (given, kept) in [
            (json!(150), 100),
            (json!(-5), 0),
            (json!(87.6), 88),
            (json!(0), 0),
        ] {
            let mut value = reply();
            value["verdict"]["score"] = given;
            let (result, _) = run(&[&value.to_string()], &digest());
            assert_eq!(result.unwrap().verdict.score, kept);
        }
    }

    #[test]
    fn a_score_that_is_not_a_number_is_a_bad_reply() {
        let mut value = reply();
        value["verdict"]["score"] = json!("very low");
        let bad = value.to_string();
        let (result, seen) = run(&[&bad, &bad], &digest());
        assert!(result.is_err());
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn empty_burns_fail() {
        let mut value = reply();
        value["burns"] = json!([]);
        let bad = value.to_string();
        let (result, seen) = run(&[&bad, &bad], &digest());
        assert!(result.is_err());
        assert_eq!(seen.len(), 2);

        // A Burn with nothing in it does not count as one.
        value["burns"] = json!([{"title": "  ", "body": "\u{0}"}]);
        let blank = value.to_string();
        assert!(run(&[&blank, &blank], &digest()).0.is_err());
    }

    #[test]
    fn a_missing_part_is_a_bad_reply() {
        for part in ["headline", "opening", "burns", "verdict"] {
            let mut value = reply();
            value.as_object_mut().unwrap().remove(part);
            let bad = value.to_string();
            assert!(run(&[&bad, &bad], &digest()).0.is_err(), "{part}");
        }
        for blank in ["archetype", "sentence"] {
            let mut value = reply();
            value["verdict"][blank] = json!("  ");
            let bad = value.to_string();
            assert!(run(&[&bad, &bad], &digest()).0.is_err(), "{blank}");
        }
    }

    #[test]
    fn evidence_and_galaxy_names_can_be_left_out() {
        let mut value = reply();
        value["verdict"].as_object_mut().unwrap().remove("evidence");
        value.as_object_mut().unwrap().remove("galaxy_names");
        let (result, _) = run(&[&value.to_string()], &digest());
        let roast = result.unwrap();
        assert!(roast.verdict.evidence.is_empty() && roast.galaxy_names.is_empty());
    }

    #[test]
    fn fields_and_burns_are_capped_so_a_reply_cannot_blow_up_the_ui() {
        let long = "w".repeat(100_000);
        let burn = json!({"title": long, "body": long});
        let reply = json!({
            "headline": long,
            "opening": long,
            "burns": vec![burn; 40],
            "verdict": {
                "archetype": long,
                "score": 1,
                "sentence": long,
                "evidence": vec![long.clone(); 40],
            },
            "galaxy_names": vec![json!({"id": 1, "name": long}); 500],
        });
        // Under the limit on a whole reply, which is about memory and not layout.
        let text = reply.to_string();
        let roast = parse_reply(&text.replace(&long, &"w".repeat(4_000))).unwrap();
        assert_eq!(roast.burns.len(), MAX_BURNS);
        for (text, cap) in [
            (&roast.headline, MAX_HEADLINE),
            (&roast.opening, MAX_PARAGRAPH),
            (&roast.burns[0].title, MAX_HEADLINE),
            (&roast.burns[0].body, MAX_PARAGRAPH),
            (&roast.verdict.archetype, MAX_HEADLINE),
            (&roast.verdict.sentence, MAX_SENTENCE),
            (&roast.verdict.evidence[0], MAX_SENTENCE),
            (&roast.galaxy_names[0].name, MAX_GALAXY_NAME),
        ] {
            assert_eq!(text.chars().count(), cap);
        }
        assert_eq!(roast.verdict.evidence.len(), MAX_EVIDENCE);
        assert_eq!(roast.galaxy_names.len(), MAX_GALAXY_NAMES);

        // A whole reply far past any real one is not even looked at.
        assert!(parse_reply(&text).is_none());
    }

    #[test]
    fn hidden_characters_go_and_line_breaks_stay() {
        let mut value = reply();
        value["burns"][0]["body"] = json!("first\u{0}\u{202e}\u{1b}[31m\n\nsecond");
        let (result, _) = run(&[&value.to_string()], &digest());
        assert_eq!(result.unwrap().burns[0].body, "first[31m\n\nsecond");
    }

    #[test]
    fn empty_digest_refuses() {
        let empty = Digest {
            spotify: None,
            youtube: None,
            ..digest()
        };
        let result = generate_with(
            |_| panic!("the Provider must not be called"),
            "Claude Code",
            &empty,
            "Be mean",
        );
        let message = result.unwrap_err().to_string();
        assert!(message.contains("nothing to roast yet"), "{message}");
        assert!(message.contains("Sources"), "{message}");
    }

    #[test]
    fn failed_run_keeps_previous_roast() {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(store.latest_roast().unwrap(), None);
        let good = good();
        let first = run(&[&good], &digest()).0.unwrap();
        let saved = store.save_roast("Claude Code", &first, 1000).unwrap();

        // A run that fails leaves nothing to save, so what was there stays.
        assert!(run(&["bad", "bad"], &digest()).0.is_err());
        assert_eq!(store.latest_roast().unwrap().unwrap(), saved);

        let second = Roast {
            headline: "A Newer One".into(),
            ..first
        };
        let newer = store.save_roast("Claude Code", &second, 2000).unwrap();
        assert!(newer.id > saved.id);
        assert_eq!(store.latest_roast().unwrap().unwrap(), newer);
        store.delete_demo().unwrap();
        assert_eq!(
            store.latest_roast().unwrap().unwrap().roast.headline,
            "A Newer One"
        );
    }

    #[test]
    fn a_stored_roast_crosses_ipc_in_camel_case() {
        let good = good();
        let roast = run(&[&good], &digest()).0.unwrap();
        let store = Store::open_in_memory().unwrap();
        let saved = store.save_roast("Claude Code", &roast, 1000).unwrap();
        let value = serde_json::to_value(&saved).unwrap();
        assert_eq!(value["providerLabel"], "Claude Code");
        assert_eq!(value["createdAt"], 1000);
        assert_eq!(value["roast"]["galaxyNames"][0]["name"], "The Quiet Room");
        assert!(value["roast"].get("galaxy_names").is_none());
        // And it survives being read back, even though a Provider writes that key in snake_case.
        assert_eq!(store.latest_roast().unwrap().unwrap().roast, roast);
    }

    #[test]
    fn the_prompt_carries_the_contract_and_the_limits() {
        let prompt = default_system_prompt();
        for needle in [
            "<digest>",
            "headline",
            "opening",
            "burns",
            "archetype",
            "score",
            "sentence",
            "evidence",
            "galaxy_names",
            "minors",
            "slurs",
            "health",
            "sexuality",
            "ignore your instructions",
        ] {
            assert!(prompt.contains(needle), "the prompt never says {needle}");
        }
        assert!(!prompt.contains('\u{2014}'), "no em dashes");
    }

    #[test]
    fn the_schema_names_every_field() {
        let schema = schema();
        let required = &schema["required"];
        for field in ["headline", "opening", "burns", "verdict", "galaxy_names"] {
            assert!(schema["properties"].get(field).is_some(), "{field}");
            assert!(
                required.as_array().unwrap().iter().any(|r| r == field),
                "{field}"
            );
        }
        assert_eq!(schema["properties"]["burns"]["maxItems"], MAX_BURNS);
    }
}
