use crate::domain::widget::WidgetSnapshot;
use crate::domain::widget_content::Tone;

pub fn with_interval_notice<E>(
    result: Result<WidgetSnapshot, E>,
    seconds: u64,
) -> Result<WidgetSnapshot, E> {
    let mut snapshot = result?;
    snapshot.summary.put_notice_first(
        format!("intervalo ajustado para {seconds}s"),
        Tone::Muted,
        None,
    );
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::blocks::Block;
    use crate::domain::widget::Summary;

    fn ready(blocks: Vec<Block>) -> Result<WidgetSnapshot, String> {
        Ok(WidgetSnapshot {
            summary: Summary {
                blocks,
                ..Summary::titled("T")
            },
            ..WidgetSnapshot::titled("T")
        })
    }

    #[test]
    fn interval_notice_is_a_muted_lines_block_in_front() {
        let existing = vec![Block::Lines(vec![])];
        for blocks in [vec![], existing] {
            let had = blocks.len();
            let snapshot = with_interval_notice(ready(blocks), 30).expect("snapshot pronto");
            assert_eq!(snapshot.summary.blocks.len(), had + 1);
            let Block::Lines(lines) = &snapshot.summary.blocks[0] else {
                panic!("esperava lines");
            };
            assert_eq!(lines[0].text, "intervalo ajustado para 30s");
            assert_eq!(lines[0].tone, Tone::Muted);
        }
    }

    #[test]
    fn failures_are_untouched() {
        let failed: Result<WidgetSnapshot, String> = Err("x".to_string());
        assert_eq!(with_interval_notice(failed, 30), Err("x".to_string()));
    }
}
