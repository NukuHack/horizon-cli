// The game adapter. All `native_*` calls live here.

use crate::game::native_hooks::{
    NativePanelEvent, NativeRecipeLayout,
    NativeRecipeView, NativeStack,
};
use crate::game::offical_native::{self as m, Event, Out, Panel, Settings};
use crate::game::Game;
use crate::game::glue_native as n;
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use std::cell::RefCell;

// ── state ─────────────────────────────────────────────────────────────

#[derive(Default)]
pub(super) struct State {
    pub settings: Option<Settings>,
    pub panel: Option<Panel>,
    pub snapshot: Option<Snapshot>,
    pub ticks: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub pos: [f64; 3],
    pub health: f32,
    pub food: i32,
    pub fps: (u32, u32, u32),
    pub target: Option<String>,
}

thread_local! {
    static STATE: RefCell<n::State> = RefCell::new(n::State::default());
}

pub(super) fn state<T>(f: impl FnOnce(&mut n::State) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

// ── settings ──────────────────────────────────────────────────────────

pub(super) fn settings(game: &Game<'_>) -> Settings {
    n::state(|s| {
        *s.settings.get_or_insert_with(|| n::Settings::read(|k| game.native_setting(m::ID, k)))
    })
}

// ── HUD ───────────────────────────────────────────────────────────────

pub(super) fn hud(game: &mut Game<'_>, batch: &mut HudBatch, corners: &mut Corners) {
    if !game.native_on(m::ID) || !game.native_playing() {
        return;
    }
    let settings = n::settings(game);
    let Some(snap) = n::state(|s| s.snapshot.clone()) else {
        return;
    };

    // A 4 px bar with the game's own `native_fill`.
    let bar = 0xFF00_FF00;
    game.native_fill(batch, [4.0, 4.0, 100.0, 4.0], bar);

    // Text, measured with `native_text_width` so it can be right-aligned.
    let line = format!(
        "{} {} {}  ❤ {:.1}  🍗 {}  {} fps",
        snap.pos[0] as i32, snap.pos[1] as i32, snap.pos[2] as i32,
        snap.health, snap.food, snap.fps.0
    );
    let w = game.native_text_width(&line);
    game.native_text(batch, &line, [batch.width - w - 6.0, 6.0], 0xFF_FFFF, true);

    if let Some(target) = &snap.target {
        let w = game.native_text_width(target);
        game.native_text(batch, target, [batch.width - w - 6.0, 18.0], 0xAA_AAAA, true);
    }

    // Item icon: `draw_item` is the same renderer every slot uses.
    let stack = ItemStack::new("minecraft:diamond", 1);
    game.draw_item(batch, &stack, 8.0, 22.0);

    // Sprite from the GUI atlas; `rect_uv` lets you slice a region.
    if let Some(sprite) = game.gui.atlas.sprites.get("gui/sprites/hud/heart/full") {
        batch.sprite(&game.gui.atlas, "gui/sprites/hud/heart/full", 30.0, 22.0);
        let _ = sprite;
    }

    // A box in a corner, with `Corners::place` so the debug screen moves it.
    if settings.overlay {
        let lines = vec![
            format!("Playground {}", m::VERSION),
            format!("Scale {}%", (settings.multiplier() * 100.0) as u32),
        ];
        let w = m::overlay_width(&lines, |t| game.native_text_width(t));
        let h = 12.0;
        let corner = Corner::TopLeft;
        if game.native_corner_shown(corner) {
            let (x, y) = corners.place(corner, w, h);
            game.native_fill(batch, [x, y, w, h], 0x9000_0000);
            let mut dy = 2.0;
            for l in &lines {
                game.native_text(batch, l, [x + 2.0, y + dy], 0xFF_FFFF, true);
                dy += 10.0;
            }
        }
    }

    // Scaling a batch region: `scale_since` is the HUD's own transform.
    let start = batch.vertices.len();
    game.native_text(batch, "scaled", [200.0, 4.0], 0xFF_FF00, true);
    batch.scale_since(start, 200.0, 4.0, settings.multiplier(), settings.multiplier());
}

// ── panel ─────────────────────────────────────────────────────────────

pub(super) fn toggle_panel(game: &mut Game<'_>) {
    let open = n::state(|s| s.panel.is_some());
    if open {
        n::state(|s| s.panel = None);
        game.native_panel_items(m::ID, Vec::new());
        return;
    }
    // In a real mod, build `Item` from `game.native_recipes()`.
    let items = vec![
        m::Item {
            id: "minecraft:stone".into(),
            name: "Stone".into(),
        },
        m::Item {
            id: "minecraft:diamond".into(),
            name: "Diamond".into(),
        },
    ];
    let (panel, out) = Panel::new(items);
    n::state(|s| s.panel = Some(panel));
    n::apply(game, out);
}

pub(super) fn convert_event(game: &Game<'_>, event: &NativePanelEvent) -> Option<Event> {
    Some(match event {
        NativePanelEvent::Search(text) => Event::Search(text.clone()),
        NativePanelEvent::Clicked { index, right } => Event::Clicked {
            index: *index as usize,
            right: *right,
        },
        NativePanelEvent::Key { key, stack } => {
            let [[recipe, _, recipe_default], [uses, _, uses_default]] = m::KEYS;
            let uses = if *key == game.native_mod_key(m::ID, recipe, recipe_default) {
                false
            } else if *key == game.native_mod_key(m::ID, uses, uses_default) {
                true
            } else {
                return None;
            };
            Event::Key {
                uses,
                stack: stack.item.clone(),
            }
        }
        NativePanelEvent::SlotClicked { stack, right, .. } => Event::SlotClicked {
            stack: stack.item.clone(),
            right: *right,
        },
        NativePanelEvent::Tab(i) => Event::Tab(*i as usize),
        NativePanelEvent::Action(i) => Event::Action(*i as usize),
        NativePanelEvent::Back => Event::Back,
        _ => return None,
    })
}

pub(super) fn apply(game: &mut Game<'_>, out: Out) {
    match out {
        Out::Items(items) => {
            let items: Vec<NativeStack> = items
                .into_iter()
                .map(|item| NativeStack {
                    item,
                    count: 1,
                    components: String::new(),
                })
                .collect();
            game.native_panel_items(m::ID, items);
        }
        Out::Recipes {
            title,
            tabs,
            tab,
            views,
        } => {
            let tabs: Vec<NativeStack> = tabs
                .into_iter()
                .map(|item| NativeStack {
                    item,
                    count: 1,
                    components: String::new(),
                })
                .collect();
            let views: Vec<NativeRecipeView> = views
                .into_iter()
                .map(|v| NativeRecipeView {
                    layout: NativeRecipeLayout::Grid,
                    width: 1,
                    height: 1,
                    slots: vec![vec![NativeStack {
                        item: v.item.clone(),
                        count: 1,
                        components: String::new(),
                    }]],
                    results: vec![NativeStack {
                        item: v.item.clone(),
                        count: v.count,
                        components: String::new(),
                    }],
                    note: v.note,
                    action: v.action,
                })
                .collect();
            game.native_panel_recipes(m::ID, &title, tabs, tab as u32, views);
        }
        Out::Give { item, count } => {
            let _ = game.native_give(&item, count);
        }
        Out::Place(book) => {
            if let Err(e) = game.native_place_recipe(book, false) {
                game.native_chat(&e);
            }
        }
        Out::Chat(text) => game.native_chat(&text),
    }
}

