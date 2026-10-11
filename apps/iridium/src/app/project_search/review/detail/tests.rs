use super::*;
#[test]
fn every_source_scalar_is_visible_once_across_pages() {
    let text = "long ".repeat(30) + "猫e\u{301}👨‍👩‍👧‍👦\\n\n\r\t\0END";
    let mut offset = 0;
    let mut rendered = String::new();
    while offset < text.len() {
        let next = page(&text, offset, 12, 2);
        assert!(next.end > offset);
        assert!(text.is_char_boundary(next.end));
        assert!(next.rows.len() <= 2);
        for row in &next.rows {
            assert!(LineLayout::new(row, 1).width() <= 12);
            rendered.push_str(row);
        }
        offset = next.end;
    }
    assert!(rendered.contains("猫e\\u{301}"), "{rendered}");
    assert!(rendered.contains("\\u{200d}"));
    assert!(rendered.ends_with("\\\\n\\n\\r\\t\\u{0}END"), "{rendered}");
    assert_eq!(rendered.matches("long ").count(), 30);
}
#[test]
fn tiny_dimensions_do_not_skip_source_and_empty_values_are_complete() {
    assert_eq!(page("abc", 0, 11, 5).end, 0);
    assert_eq!(page("abc", 0, 100, 0).end, 0);
    assert_eq!(page("", 0, 100, 10).end, 0);
    assert_eq!(page("12345678901猫", 0, 12, 1).end, 11);
    let next = page("12345678901猫", 11, 12, 1);
    assert_eq!(next.rows, ["猫"]);
    assert_eq!(next.end, 14);
}
#[test]
#[ignore = "manual bounded-page performance measurement"]
fn measured_megabyte_value_inspection() {
    use std::{io::Write as _, time::Instant};
    let text = "猫".repeat(349_525);
    let mut samples = Vec::new();
    for _ in 0..101 {
        let start = Instant::now();
        let result = page(&text, 0, 100, 20);
        std::hint::black_box(result);
        samples.push(start.elapsed().as_micros());
    }
    samples.sort_unstable();
    writeln!(
        std::io::stdout().lock(),
        "REVIEW_DETAIL bytes={} median_us={} max_us={}",
        text.len(),
        samples[50],
        samples[100]
    )
    .expect("report");
}
