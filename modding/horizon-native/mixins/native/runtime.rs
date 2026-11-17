// Pure logic: settings, the panel's state machine, and layout maths.
// The game adapter lives in `glue.rs`.

use std::collections::VecDeque;

#[allow(dead_code)]
pub const ID: &str = "native";
#[allow(dead_code)]
pub const NAME: &str = "Native";
#[allow(dead_code)]
pub const VERSION: &str = "0.1.1";
#[allow(dead_code)]
pub const DESCRIPTION: &str = "One mod that exercises some native hook point: HUD, panel, keys, wheel, zoom, tick, tooltip, settings, give, place recipe.";

/// `native_mod_key` names and defaults.
pub const KEYS: [[&str; 3]; 2] = [
    ["panel", "Toggle panel", "key.keyboard.p"],
    ["overlay", "Toggle overlay", "key.keyboard.o"],
];

pub const OVERLAY_KEY: &str = "overlay";
pub const SCALE_KEY: &str = "scale";
pub const SKIP_PACKS_KEY: &str = "skip_packs";

// ── settings ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub overlay: bool,
    /// 0..=3, multiplied by 25 %.
    pub scale: usize,
    pub skip_packs: bool, // NEW
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            overlay: true,
            scale: 1,
            skip_packs: true,
        }
    }
}

impl Settings {
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let overlay = get(OVERLAY_KEY)
            .map(|v| v.trim() != "false")
            .unwrap_or(true);
        let scale = get(SCALE_KEY)
            .and_then(|v| v.trim().parse().ok())
            .map_or(1, |n: usize| n.min(3));
        let skip_packs = get(SKIP_PACKS_KEY)
            .map(|v| v.trim() != "false")
            .unwrap_or(true);
        Self {
            overlay,
            scale,
            skip_packs,
        }
    }

    pub fn stored(&self) -> [(&'static str, String); 3] {
        [
            (OVERLAY_KEY, self.overlay.to_string()),
            (SCALE_KEY, self.scale.to_string()),
            (SKIP_PACKS_KEY, self.skip_packs.to_string()),
        ]
    }

    pub fn labels(&self) -> Vec<String> {
        vec![
            format!("Overlay: {}", on_off(self.overlay)),
            format!("Scale: {}%", self.scale * 25 + 100),
            format!("Skip server packs: {}", on_off(self.skip_packs)),
            "Reset".into(),
        ]
    }

    pub fn click(&mut self, button: usize) -> bool {
        match button {
            0 => {
                self.overlay = !self.overlay;
                true
            }
            1 => {
                self.scale = (self.scale + 1) % 4;
                true
            }
            2 => {
                self.skip_packs = !self.skip_packs;
                true
            }
            3 => {
                *self = Self::default();
                true
            }
            _ => false,
        }
    }

    /// The HUD scale multiplier (1.0 = 100 %).
    pub fn multiplier(&self) -> f32 {
        1.0 + self.scale as f32 * 0.25
    }
}

fn on_off(v: bool) -> &'static str {
    if v { "ON" } else { "OFF" }
}

// ── panel ─────────────────────────────────────────────────────────────

/// An item the panel can show. In the real game this comes from
/// `game.native_recipes()`; here it is plain data.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    pub name: String,
}

/// What the panel is showing.
#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Items {
        search: String,
        listed: Vec<usize>,
    },
    Recipes {
        item: String,
        uses: bool,
        tab: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Search(String),
    Clicked { index: usize, right: bool },
    Key { uses: bool, stack: String },
    SlotClicked { stack: String, right: bool },
    Tab(usize),
    Action(usize),
    Back,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    Items(Vec<String>),
    Recipes {
        title: String,
        tabs: Vec<String>,
        tab: usize,
        views: Vec<View>,
    },
    Give {
        item: String,
        count: u32,
    },
    Place(i32),
    Chat(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub item: String,
    pub count: u32,
    pub note: String,
    pub action: Option<bool>,
}

/// The panel's state: the same shape the game's native panel uses.
#[derive(Default)]
pub struct Panel {
    pub items: Vec<Item>,
    pub page: Option<Page>,
    pub history: VecDeque<String>,
}

impl Panel {
    pub fn new(items: Vec<Item>) -> (Self, Out) {
        let mut me = Self {
            items,
            ..Self::default()
        };
        let out = me.list("");
        (me, out)
    }

    fn list(&mut self, search: &str) -> Out {
        let words: Vec<String> = search
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();
        let listed: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                words.iter().all(|w| {
                    i.name.to_lowercase().contains(w.as_str()) || i.id.contains(w.as_str())
                })
            })
            .map(|(n, _)| n)
            .collect();
        self.page = Some(Page::Items {
            search: search.into(),
            listed: listed.clone(),
        });
        Out::Items(listed.iter().map(|&n| self.items[n].id.clone()).collect())
    }

    fn recipes_for(&mut self, item: &str, uses: bool) -> Out {
        // One hardcoded tab and one hardcoded view is enough to show the API.
        let tab = 0;
        let title = format!(
            "{}: {}",
            if uses { "Uses" } else { "Recipes" },
            self.items
                .iter()
                .find(|i| i.id == item)
                .map_or(item, |i| &i.name)
        );
        let views = vec![View {
            item: item.into(),
            count: 1,
            note: if uses {
                "used by Playground".into()
            } else {
                "made by Playground".into()
            },
            action: Some(true),
        }];
        self.history.push_back(item.into());
        Out::Recipes {
            title,
            tabs: vec!["minecraft:crafting_table".into()],
            tab,
            views,
        }
    }

    /// `creative` is whether the game would actually give an item.
    pub fn handle(&mut self, event: Event, creative: bool) -> Vec<Out> {
        match event {
            Event::Search(text) => vec![self.list(&text)],
            Event::Clicked { index, right } => {
                let Page::Items { listed, .. } = self.page.clone().unwrap_or(Page::Items {
                    search: String::new(),
                    listed: Vec::new(),
                }) else {
                    return Vec::new();
                };
                let Some(&n) = listed.get(index) else {
                    return Vec::new();
                };
                let item = self.items[n].id.clone();
                if creative {
                    return vec![Out::Give {
                        item,
                        count: if right { 64 } else { 1 },
                    }];
                }
                vec![self.recipes_for(&item, right)]
            }
            Event::Key { uses, stack } => vec![self.recipes_for(&stack, uses)],
            Event::SlotClicked { stack, right } => {
                vec![self.recipes_for(&stack, right)]
            }
            Event::Tab(i) => {
                if let Some(Page::Recipes { tab, .. }) = &mut self.page {
                    *tab = i;
                }
                vec![Out::Chat(format!("tab {i}"))]
            }
            Event::Action(i) => {
                // A real mod would read `self.shown[i]`; here every action
                // places recipe-book id 0.
                let _ = i;
                vec![Out::Place(0)]
            }
            Event::Back => {
                self.history.pop_back();
                match self.history.back().cloned() {
                    Some(item) => vec![self.recipes_for(&item, false)],
                    None => vec![self.list("")],
                }
            }
        }
    }
}

// ── layout helpers ────────────────────────────────────────────────────

/// Box `w` × `h` in a corner, 2 px from the edges.
pub fn place_in_corner(corner: usize, w: f32, h: f32, gw: f32, gh: f32) -> (f32, f32) {
    match corner {
        0 => (2.0, 2.0),
        1 => (gw - w - 2.0, 2.0),
        2 => (2.0, gh - h - 2.0),
        _ => (gw - w - 2.0, gh - h - 2.0),
    }
}

/// The overlay's width, from its longest line.
pub fn overlay_width(lines: &[String], measure: impl Fn(&str) -> f32) -> f32 {
    lines.iter().map(|l| measure(l)).fold(0.0, f32::max) + 4.0
}

// ── tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<Item> {
        vec![
            Item {
                id: "minecraft:stone".into(),
                name: "Stone".into(),
            },
            Item {
                id: "minecraft:dirt".into(),
                name: "Dirt".into(),
            },
        ]
    }

    #[test]
    fn settings_round_trip() {
        let s = Settings::read(|_| None);
        assert_eq!(s, Settings::default());
        assert_eq!(
            Settings::read(|k| (k == SCALE_KEY).then(|| "2".into())).scale,
            2
        );
        let mut s = Settings::default();
        s.click(1);
        assert_eq!(s.scale, 2);
        s.click(2);
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn panel_lists_and_filters() {
        let (mut p, out) = Panel::new(items());
        assert_eq!(
            out,
            Out::Items(vec!["minecraft:stone".into(), "minecraft:dirt".into()])
        );
        let filtered = p.handle(Event::Search("dirt".into()), false);
        assert_eq!(filtered, vec![Out::Items(vec!["minecraft:dirt".into()])]);
    }

    #[test]
    fn panel_gives_in_creative_and_opens_a_page_otherwise() {
        let (mut p, _) = Panel::new(items());
        assert_eq!(
            p.handle(
                Event::Clicked {
                    index: 0,
                    right: false
                },
                true
            ),
            vec![Out::Give {
                item: "minecraft:stone".into(),
                count: 1
            }]
        );
        let page = p.handle(
            Event::Clicked {
                index: 0,
                right: true,
            },
            false,
        );
        assert!(matches!(&page[0], Out::Recipes { .. }));
    }

    #[test]
    fn back_returns_to_the_list() {
        let (mut p, _) = Panel::new(items());
        p.handle(
            Event::Key {
                uses: false,
                stack: "minecraft:stone".into(),
            },
            false,
        );
        let back = p.handle(Event::Back, false);
        assert!(matches!(&back[0], Out::Items(_)));
    }
}
