use super::*;

#[test]
fn short_pane_keeps_disclosure_and_exact_destination() {
    let creative = default_ad_creative();
    let lines = ad_lines(&creative, 80, 6, 0);
    let text = lines.join("\n");
    assert!(text.contains("Sponsored preview: RAILWAY"));
    assert!(text.contains(&creative.url));
    assert!(text.contains(&format!("[{}]", creative.disclosure)));
    assert!(lines.len() <= 6);
    assert!(!full_creative_fits(&creative, Layout::new(80, 6)));
}
