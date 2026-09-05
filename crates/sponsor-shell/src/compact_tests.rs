use super::*;

#[test]
fn preview_preserves_only_complete_logo_variants() {
    let mut creative = default_ad_creative();
    creative.logos.small = vec!["[LOGO]".into(), "[BASE]".into()];
    let short = ad_lines(&creative, 80, 6, 0).join("\n");
    assert!(!short.contains("[LOGO]"));
    assert!(!short.contains("[BASE]"));
    let taller = ad_lines(&creative, 80, 7, 0).join("\n");
    assert!(taller.contains("[LOGO]"));
    assert!(taller.contains("[BASE]"));
    assert!(taller.contains(&creative.url));
    assert!(taller.contains(&creative.disclosure));
}

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
