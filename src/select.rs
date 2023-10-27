use regex::Regex;
use skim::prelude::*;

#[allow(dead_code)]
pub fn select_with_preview<E: SkimItem + Clone>(entries: &[E]) -> Vec<E> {
    if entries.len() <= 1 {
        entries.to_vec()
    } else {
        select_via_builder(
            entries,
            SkimOptionsBuilder::default()
                .multi(true)
                .preview_window(Some("up:50%:wrap"))
                .preview(Some("")),
        )
    }
}

#[allow(dead_code)]
pub fn select_with_regex<E: SkimItem + Clone>(
    entries: &[E],
    regex: &str,
) -> Result<Vec<E>, regex::Error> {
    let re = Regex::new(regex)?;
    let filtered = entries
        .iter()
        .cloned()
        .filter(|e| re.is_match(&e.text()))
        .collect();
    Ok(filtered)
}

#[allow(dead_code)]
pub fn select_plain<E: SkimItem + Clone>(entries: &[E]) -> Vec<E> {
    select_via_builder(entries, SkimOptionsBuilder::default().multi(true))
}

#[allow(dead_code)]
pub fn select_plain_single<E: SkimItem + Clone>(mut entries: Vec<E>) -> Option<E> {
    match entries.len() {
        0..=1 => entries.pop(),
        _ => select_via_builder(&entries[..], SkimOptionsBuilder::default().multi(false)).pop(),
    }
}

#[allow(dead_code)]
fn select_via_builder<E: SkimItem + Clone>(
    entries: &[E],
    builder: &mut SkimOptionsBuilder,
) -> Vec<E> {
    let options = builder
        .case(skim::CaseMatching::Smart)
        .exact(true)
        .build()
        .expect("invalid skim options");

    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = unbounded();
    for entry in entries.iter().cloned() {
        tx_item
            .send(Arc::new(entry))
            .expect("could not sent entry to skim");
    }
    drop(tx_item); // so that skim could know when to stop waiting for more items.
    Skim::run_with(&options, Some(rx_item))
        .and_then(|out| {
            if !out.is_abort {
                Some(out.selected_items)
            } else {
                None
            }
        })
        .unwrap_or_default()
        .iter()
        .map(|selected_item| {
            (**selected_item)
                .as_any()
                .downcast_ref::<E>()
                .expect("failed to downcast")
                .to_owned()
        })
        .collect::<Vec<E>>()
}
