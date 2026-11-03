//! Builds a Share Site from demo data and a canned Roast, for e2e tests and for a look by eye.
//!
//! `cargo run -p rostery-core --example demo_site -- dist/share dist/demo-site [--hostile]`
//!
//! `--hostile` adds names, titles and Roast text written to break out of the page.

use rostery_core::demo;
use rostery_core::digest;
use rostery_core::export::{self, ExportOptions};
use rostery_core::model::{Origin, Play, Source};
use rostery_core::roast::{self, Roast};
use rostery_core::store::Store;
use serde_json::json;
use std::path::PathBuf;

const NOW: i64 = 1_800_000_000;
const ATTACK: &str = "</script><img src=x onerror=\"window.ran=1\"><script>window.ran=1</script>";

fn main() -> rostery_core::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (template, out) = match &args[..] {
        [template, out, ..] => (PathBuf::from(template), PathBuf::from(out)),
        _ => {
            eprintln!("usage: demo_site <dist/share> <out dir> [--hostile]");
            std::process::exit(2);
        }
    };
    let hostile = args.iter().any(|a| a == "--hostile");

    let mut store = Store::open_in_memory()?;
    demo::load(&mut store, NOW)?;
    if hostile {
        let play = |source, creator: &str| Play {
            source,
            origin: Origin::Demo,
            played_at: NOW - 3600,
            duration_ms: Some(240_000),
            creator: creator.into(),
            creator_ref: None,
            item: ATTACK.into(),
            item_ref: None,
            skipped: false,
        };
        let plays: Vec<Play> = (0..6)
            .flat_map(|_| [play(Source::Spotify, ATTACK), play(Source::Youtube, ATTACK)])
            .collect();
        store.insert_plays(&plays)?;
    }

    let words = if hostile { ATTACK } else { "Moth Season" };
    let canned: Roast = serde_json::from_value(json!({
        "headline": format!("Three hundred nights with {words}"),
        "opening": format!("You have played {words} more than you have called your mother. We counted."),
        "burns": [
            {"title": "A standing midnight appointment", "body": format!("22 percent of your plays land after midnight, and {words} is always there.")},
            {"title": "Soup, again", "body": "Fixing Sunday Soup, twenty times. The soup is still broken."},
            {"title": "Eighty songs judged", "body": "You skipped 80 songs and kept the one that sounds like a fridge."},
        ],
        "verdict": {
            "archetype": if hostile { ATTACK } else { "The Night Clerk" },
            "score": 72,
            "sentence": "Go to bed.",
            "evidence": ["22 percent of plays after midnight", "Fixing Sunday Soup, 20 times"],
        },
        "galaxy_names": [{"id": 0, "name": "The Quiet Room"}, {"id": 1, "name": if hostile { ATTACK } else { "Kitchen Radio" }}],
    }))?;
    let digest = digest::from_store(&store, NOW)?;
    store.save_roast("Claude Code", &roast::anchored(canned, &digest), NOW, true)?;

    let opts = ExportOptions {
        display_name: if hostile { ATTACK.into() } else { "Sam".into() },
        include_item_titles: true,
        ..ExportOptions::default()
    };
    let payload = export::build_payload(&store, &opts, NOW)?;
    let files = export::render_site(&export::read_template(&template)?, &payload)?;
    export::write_site(&files, &out)?;
    println!("wrote {}", out.display());
    Ok(())
}
