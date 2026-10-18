use crate::domain::card::Tone;
use crate::domain::widget::WidgetState;

pub fn with_interval_notice(state: WidgetState, seconds: u64) -> WidgetState {
    let WidgetState::Ready(mut snapshot) = state else {
        return state;
    };
    snapshot.summary.put_notice_first(
        format!("intervalo ajustado para {seconds}s"),
        Tone::Muted,
        None,
    );
    WidgetState::Ready(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::card::{Block, Tone};
    use crate::domain::widget::{Summary, WidgetSnapshot};

    fn ready(blocks: Vec<Block>) -> WidgetState {
        WidgetState::Ready(WidgetSnapshot {
            summary: Summary {
                title: "T".to_string(),
                lines: vec![],
                blocks,
            },
            detail: None,
            actions: vec![],
            alerts: vec![],
        })
    }

    #[test]
    fn ready_state_gets_the_notice_as_first_line() {
        let WidgetState::Ready(snapshot) = with_interval_notice(ready(vec![]), 30) else {
            panic!("esperava Ready");
        };
        assert_eq!(
            snapshot.summary.lines[0].text,
            "intervalo ajustado para 30s"
        );
    }

    #[test]
    fn notice_is_a_muted_block_when_the_card_uses_blocks() {
        let WidgetState::Ready(snapshot) =
            with_interval_notice(ready(vec![Block::Lines(vec![])]), 30)
        else {
            panic!("esperava Ready");
        };
        let Block::Lines(lines) = &snapshot.summary.blocks[0] else {
            panic!("esperava lines");
        };
        assert_eq!(lines[0].tone, Tone::Muted);
    }

    #[test]
    fn loading_and_failed_states_are_untouched() {
        assert_eq!(
            with_interval_notice(WidgetState::Loading, 30),
            WidgetState::Loading
        );
        let failed = WidgetState::Failed("x".to_string());
        assert_eq!(with_interval_notice(failed.clone(), 30), failed);
    }
}
