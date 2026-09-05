use super::*;

#[test]
fn cramped_preview_is_hidden_instead_of_cropping_a_link() {
    let creative = default_ad_creative();
    for (cols, rows) in [(80, 4), (12, 10), (48, 3)] {
        let lines = ad_lines(&creative, cols, rows, 0);
        assert!(lines.len() <= usize::from(rows));
        assert!(!lines.join("\n").contains(&creative.url));
        assert!(!lines.join("\n").contains(&creative.sponsor));
        for row in 0..rows {
            for col in 0..cols {
                assert_eq!(link_at_cell(&creative, Layout::new(cols, rows), 0, row, col), None);
            }
        }
    }
}

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
