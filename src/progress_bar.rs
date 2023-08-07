use indicatif::ProgressStyle;

/// Progress bar style for counts
pub fn style_progress_bar_count() -> indicatif::ProgressStyle {
    ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] {msg}[{bar:40.cyan/blue}] {pos} / {len} \
                   @ {per_sec} ({eta})",
        )
        .unwrap()
        .progress_chars("#>-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbar() {
        style_progress_bar_count();
    }
}
