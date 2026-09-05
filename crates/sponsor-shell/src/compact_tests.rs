use super::*;

#[test]
fn full_creative_keeps_original_signed_geometry_and_minimum_duration() {
    let mut creative = default_ad_creative();
    creative.ad_decision_id = Some("full-decision".into());
    creative.decision_token = Some("fixture-token".into());
    let mut pending = Vec::new();
    let mut sequence = 1;
    let layout = Layout::new(80, ad_height(&creative, 80));
    assert!(full_creative_fits(&creative, layout));
    assert!(!enqueue_impression_if_needed(
        &creative,
        &HashSet::new(),
        &HashSet::new(),
        &mut pending,
        layout,
        &mut sequence,
        999
    ));
    assert!(enqueue_impression_if_needed(
        &creative,
        &HashSet::new(),
        &HashSet::new(),
        &mut pending,
        layout,
        &mut sequence,
        1_000
    ));
    let body: serde_json::Value = serde_json::from_str(&pending[0].body).unwrap();
    assert_eq!(body["lineCount"], ad_height(&creative, 80));
    assert_eq!(body["decisionToken"], "fixture-token");
    assert_eq!(body["visibleDurationMs"], 1_000);
    assert_eq!(sequence, 2);
}

#[test]
fn short_panes_never_queue_or_consume_an_impression_sequence() {
    let mut creative = default_ad_creative();
    creative.ad_decision_id = Some("signed-preview".into());
    creative.decision_token = Some("fixture-token".into());
    let mut pending = Vec::new();
    let mut sequence = 7;
    for rows in [1, 2, 4, 6] {
        assert!(!enqueue_impression_if_needed(
            &creative,
            &HashSet::new(),
            &HashSet::new(),
            &mut pending,
            Layout::new(80, rows),
            &mut sequence,
            60_000
        ));
    }
    assert!(pending.is_empty());
    assert_eq!(sequence, 7);
}

#[test]
fn short_preview_link_is_navigable_and_footer_has_no_hit_target() {
    let creative = default_ad_creative();
    let layout = Layout::new(80, 6);
    let mut lines = ad_lines(&creative, layout.cols, layout.rows, 0);
    let row = lines
        .iter()
        .position(|line| line.contains(&creative.url))
        .unwrap();
    let col = lines[row].find(&creative.url).unwrap() + 4;
    assert_eq!(
        link_at_cell(&creative, layout, 0, row as u16, col as u16),
        Some(creative.url.clone())
    );
    assert!(linkified_line(&lines[row], &creative, None).contains("\x1b]8;;https://railway.app"));
    let footer = lines.len() - 1;
    add_activity_footer(&mut lines, layout.cols, "Claude | activity unavailable");
    assert!(lines[row].contains(&creative.url));
    for col in 0..layout.cols {
        assert_eq!(link_at_cell(&creative, layout, 0, footer as u16, col), None);
    }
}

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
                assert_eq!(
                    link_at_cell(&creative, Layout::new(cols, rows), 0, row, col),
                    None
                );
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
