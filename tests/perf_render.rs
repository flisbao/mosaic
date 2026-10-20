#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Medições de desempenho do render. Todos os testes são `#[ignore]`: rodam só sob demanda.
//! Uso: `cargo test --release --test perf_render -- --ignored --nocapture --test-threads=1`

use std::time::{Duration, Instant};

use chrono::NaiveDateTime;
use mosaic::domain::blocks::{lines::WidgetLine, Block};
use mosaic::domain::widget::{
    DetailItem, DetailSection, Summary, WidgetId, WidgetRecord, WidgetSnapshot,
};
use mosaic::domain::widget_content::Tone;
use mosaic::presentation::app_state::{AppState, TabView, WidgetEntry};
use mosaic::presentation::key_input::KeyInput;
use mosaic::presentation::render::render;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const TERMINAL_WIDTH: u16 = 200;
const TERMINAL_HEIGHT: u16 = 50;
const WARMUP_FRAMES: u32 = 2;
const MEASURED_FRAMES: u32 = 20;
const MEASURED_KEYS: u32 = 2_000;
const SECTION_COUNT: usize = 100;
const HOME_WIDGET_COUNT: usize = 12;
const HOME_BLOCKS_PER_WIDGET: usize = 20;
const HOME_LINES_PER_BLOCK: usize = 200;
const HOME_LINE_CHARACTERS: usize = 200;
const WIDGET_INTERVAL: Duration = Duration::from_secs(30);

fn instant() -> NaiveDateTime {
    NaiveDateTime::parse_from_str("2026-01-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
}

fn plain_summary(title: &str, blocks: Vec<Block>) -> Summary {
    Summary {
        title: title.to_string(),
        blocks,
    }
}

fn snapshot(summary: Summary, detail: Option<Vec<DetailSection>>) -> WidgetSnapshot {
    WidgetSnapshot {
        summary,
        detail,
        actions: vec![],
        alerts: vec![],
        mini: None,
    }
}

fn entry(id: &str, snapshot: WidgetSnapshot) -> WidgetEntry {
    let mut record = WidgetRecord::loading();
    record.apply(Ok::<_, String>(snapshot), instant());
    WidgetEntry {
        id: WidgetId::new(id),
        record,
        interval: WIDGET_INTERVAL,
        max_items: None,
    }
}

fn state_of(entries: Vec<WidgetEntry>) -> AppState {
    let tab = TabView {
        name: "tab".to_string(),
        entry_indexes: (0..entries.len()).collect(),
    };
    AppState::new(entries, vec![tab])
}

fn detail_state(items_per_section: usize) -> AppState {
    let sections = (0..SECTION_COUNT)
        .map(|section| DetailSection {
            heading: format!("secao {section}"),
            items: (0..items_per_section)
                .map(|item| DetailItem {
                    id: Some(format!("{section}-{item}")),
                    text: format!("item {item} da secao {section} com texto de tamanho médio"),
                    flag: None,
                    meta: vec![],
                })
                .collect(),
            blocks: vec![],
        })
        .collect();
    let summary = plain_summary("detalhe", vec![]);
    state_of(vec![entry("detalhe", snapshot(summary, Some(sections)))])
}

fn home_state() -> AppState {
    let entries = (0..HOME_WIDGET_COUNT)
        .map(|index| {
            let blocks = (0..HOME_BLOCKS_PER_WIDGET)
                .map(|_| {
                    Block::Lines(
                        (0..HOME_LINES_PER_BLOCK)
                            .map(|_| WidgetLine {
                                text: "x".repeat(HOME_LINE_CHARACTERS),
                                tone: Tone::Normal,
                                flag: None,
                            })
                            .collect(),
                    )
                })
                .collect();
            let summary = plain_summary(&format!("widget {index}"), blocks);
            entry(&format!("w{index}"), snapshot(summary, None))
        })
        .collect();
    state_of(entries)
}

fn milliseconds_per_frame(state: &AppState) -> f64 {
    let mut terminal = Terminal::new(TestBackend::new(TERMINAL_WIDTH, TERMINAL_HEIGHT)).unwrap();
    let spans = vec![1u16; state.entries().len()];
    let mut draw = || {
        terminal
            .draw(|frame| render(frame, state, &spans, instant()))
            .unwrap();
    };
    for _ in 0..WARMUP_FRAMES {
        draw();
    }
    let started = Instant::now();
    for _ in 0..MEASURED_FRAMES {
        draw();
    }
    started.elapsed().as_secs_f64() * 1000.0 / f64::from(MEASURED_FRAMES)
}

#[test]
#[ignore]
fn focus_mode_frame_time_with_100000_items() {
    let mut state = detail_state(1_000);
    state.handle_key(KeyInput::Enter);
    let milliseconds = milliseconds_per_frame(&state);
    println!("PERF focus 100000 itens: {milliseconds:.3} ms/frame");
    assert!(milliseconds < 5.0, "{milliseconds:.3} ms/frame");
}

#[test]
#[ignore]
fn focus_mode_frame_time_with_selection_at_the_end_of_100000_items() {
    let mut state = detail_state(1_000);
    state.handle_key(KeyInput::Enter);
    state.handle_key(KeyInput::End);
    let milliseconds = milliseconds_per_frame(&state);
    println!("PERF focus 100000 itens, seleção no fim: {milliseconds:.3} ms/frame");
    assert!(milliseconds < 5.0, "{milliseconds:.3} ms/frame");
}

#[test]
#[ignore]
fn focus_mode_frame_time_with_10000_items() {
    let mut state = detail_state(100);
    state.handle_key(KeyInput::Enter);
    let milliseconds = milliseconds_per_frame(&state);
    println!("PERF focus 10000 itens: {milliseconds:.3} ms/frame");
    assert!(milliseconds < 1.0, "{milliseconds:.3} ms/frame");
}

#[test]
#[ignore]
fn home_frame_time_with_12_full_widgets() {
    let state = home_state();
    let milliseconds = milliseconds_per_frame(&state);
    println!("PERF home 12 widgets cheios: {milliseconds:.3} ms/frame");
    assert!(milliseconds < 3.0, "{milliseconds:.3} ms/frame");
}

#[test]
#[ignore]
fn key_down_time_with_100000_items() {
    let mut state = detail_state(1_000);
    state.handle_key(KeyInput::Enter);
    let started = Instant::now();
    for _ in 0..MEASURED_KEYS {
        state.handle_key(KeyInput::Down);
    }
    let microseconds = started.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(MEASURED_KEYS);
    println!("PERF handle_key(Down) 100000 itens: {microseconds:.2} us/tecla");
    assert!(microseconds < 5.0, "{microseconds:.2} us/tecla");
}
