//! Little companions that wander along the bottom of the window.
//!
//! Sprites: "Farm RPG" asset pack by EmanuelleDev (emanuelledev.itch.io).
//! They are NOT covered by Hazelnut's MIT license; see README.

use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions, pos2, vec2};
use std::collections::HashMap;

/// On-screen size of one sprite pixel, in points.
pub const PIXEL: f32 = 2.0;
/// One animation: a row of frames in a sheet.
#[derive(Clone, Copy)]
pub struct Anim {
    /// Index into `Species::sheets`.
    pub sheet: usize,
    pub row: u32,
    pub frames: u32,
    pub fps: f32,
    /// Loop, or play once and hold the last frame.
    pub looped: bool,
}

const fn anim(sheet: usize, row: u32, frames: u32, fps: f32, looped: bool) -> Anim {
    Anim { sheet, row, frames, fps, looped }
}

pub struct Species {
    pub id: &'static str,
    pub name: &'static str,
    pub sheets: &'static [&'static [u8]],
    /// Frame size in sprite pixels (frames are square).
    pub cell: u32,
    /// Direction the sprites face in the sheet.
    pub faces_right: bool,
    /// Walking speed, in points per second.
    pub speed: f32,
    pub walk: Anim,
    pub idle: Anim,
    pub sit: Option<Anim>,
    pub sleep: Option<Anim>,
}

macro_rules! sheet {
    ($path:literal) => {
        include_bytes!(concat!("../assets/pets/Animals/", $path)) as &[u8]
    };
}

// Layouts shared by the cat and dog sheets.
const PET_WALK: Anim = anim(0, 0, 4, 8.0, true);
const PET_IDLE: Anim = anim(0, 0, 1, 1.0, true);
const PET_SIT: Anim = anim(0, 3, 4, 6.0, false);
const CAT_SLEEP: Anim = anim(0, 9, 4, 2.0, true);
const DOG_SLEEP: Anim = anim(0, 8, 4, 2.0, true);

const fn cat(id: &'static str, name: &'static str, sheets: &'static [&'static [u8]]) -> Species {
    Species {
        id,
        name,
        sheets,
        cell: 32,
        faces_right: false,
        speed: 26.0,
        walk: PET_WALK,
        idle: PET_IDLE,
        sit: Some(PET_SIT),
        sleep: Some(CAT_SLEEP),
    }
}

const fn dog(id: &'static str, name: &'static str, sheets: &'static [&'static [u8]]) -> Species {
    Species {
        id,
        name,
        sheets,
        cell: 32,
        faces_right: false,
        speed: 32.0,
        walk: PET_WALK,
        idle: PET_IDLE,
        sit: Some(PET_SIT),
        sleep: Some(DOG_SLEEP),
    }
}

const fn rabbit(id: &'static str, name: &'static str, sheets: &'static [&'static [u8]]) -> Species {
    Species {
        id,
        name,
        sheets,
        cell: 16,
        faces_right: false,
        speed: 34.0,
        walk: anim(0, 3, 4, 10.0, true),
        idle: anim(0, 0, 4, 3.0, true),
        sit: None,
        sleep: Some(anim(0, 6, 4, 1.5, true)),
    }
}

const fn deer(id: &'static str, name: &'static str, sheets: &'static [&'static [u8]]) -> Species {
    // Sheets: walk, idle, sleep.
    Species {
        id,
        name,
        sheets,
        cell: 32,
        faces_right: false,
        speed: 28.0,
        walk: anim(0, 0, 4, 8.0, true),
        idle: anim(1, 0, 2, 1.5, true),
        sit: None,
        sleep: Some(anim(2, 0, 2, 1.0, true)),
    }
}

const fn frog(id: &'static str, name: &'static str, sheets: &'static [&'static [u8]]) -> Species {
    Species {
        id,
        name,
        sheets,
        cell: 16,
        faces_right: true,
        speed: 26.0,
        walk: anim(0, 1, 8, 10.0, true),
        idle: anim(0, 4, 4, 3.0, true),
        // Croaking.
        sit: Some(anim(0, 5, 6, 5.0, true)),
        sleep: None,
    }
}

/// Pigs and sheep (adults and young) share a layout.
const fn grazer(id: &'static str, name: &'static str, speed: f32, sheets: &'static [&'static [u8]]) -> Species {
    Species {
        id,
        name,
        sheets,
        cell: 32,
        faces_right: false,
        speed,
        walk: anim(0, 0, 4, 7.0, true),
        idle: anim(0, 0, 1, 1.0, true),
        // Lying down.
        sit: Some(anim(0, 5, 4, 5.0, false)),
        sleep: Some(anim(0, 3, 4, 2.0, true)),
    }
}

/// The companions on offer.
pub static SPECIES: &[Species] = &[
    Species {
        id: "fox",
        name: "Fox",
        sheets: &[sheet!("Forest/Fox/Red Fox.png")],
        cell: 32,
        faces_right: false,
        speed: 30.0,
        walk: anim(0, 0, 4, 8.0, true),
        idle: anim(0, 0, 1, 1.0, true),
        sit: Some(anim(0, 3, 4, 6.0, false)),
        sleep: Some(anim(0, 5, 4, 2.0, true)),
    },
    rabbit("rabbit", "Rabbit", &[sheet!("Forest/Rabbit/Rabbit Brown.png")]),
    rabbit("blonde-rabbit", "Blonde rabbit", &[sheet!("Forest/Rabbit/Rabbit Blonde.png")]),
    deer(
        "stag",
        "Stag",
        &[sheet!("Forest/Deer/Male/Walk.png"), sheet!("Forest/Deer/Male/Idle.png"), sheet!("Forest/Deer/Male/Sleep.png")],
    ),
    deer(
        "doe",
        "Doe",
        &[sheet!("Forest/Deer/Female/Walk.png"), sheet!("Forest/Deer/Female/Idle.png"), sheet!("Forest/Deer/Female/sleep.png")],
    ),
    frog("frog", "Frog", &[sheet!("Forest/Frog/Frogs-Sheet.png")]),
    frog("yellow-frog", "Yellow frog", &[sheet!("Forest/Frog/Yellow.png")]),
    frog("pink-frog", "Pink frog", &[sheet!("Forest/Frog/Pink.png")]),
    Species {
        id: "turtle",
        name: "Turtle",
        // Walk, idle, hide.
        sheets: &[
            sheet!("Forest/Turttle/Green/Walk.png"),
            sheet!("Forest/Turttle/Green/Idle.png"),
            sheet!("Forest/Turttle/Green/Sleep, hide.png"),
        ],
        cell: 32,
        faces_right: true,
        speed: 10.0,
        walk: anim(0, 0, 4, 5.0, true),
        idle: anim(1, 0, 2, 2.0, true),
        sit: None,
        sleep: Some(anim(2, 0, 4, 5.0, false)),
    },
    Species {
        id: "capybara",
        name: "Capybara",
        sheets: &[sheet!("Forest/Capybara/Brown Capybara.png")],
        cell: 32,
        faces_right: false,
        speed: 18.0,
        walk: anim(0, 0, 4, 6.0, true),
        idle: anim(0, 0, 1, 1.0, true),
        sit: Some(anim(0, 3, 4, 4.0, true)),
        sleep: Some(anim(0, 7, 4, 2.0, true)),
    },
    Species {
        id: "penguin",
        name: "Penguin",
        sheets: &[sheet!("Forest/Penguin/Penguin.png")],
        cell: 16,
        faces_right: true,
        speed: 18.0,
        walk: anim(0, 1, 4, 8.0, true),
        idle: anim(0, 0, 4, 3.0, true),
        sit: None,
        sleep: None,
    },
    Species {
        id: "hen",
        name: "Hen",
        sheets: &[sheet!("Farm/Chicken/Chicken Brown White.png")],
        cell: 16,
        faces_right: false,
        speed: 20.0,
        walk: anim(0, 0, 4, 8.0, true),
        idle: anim(0, 1, 4, 3.0, true),
        // Pecking at the ground.
        sit: Some(anim(0, 3, 4, 6.0, true)),
        // Settled down as if on a nest.
        sleep: Some(anim(0, 4, 4, 2.0, true)),
    },
    Species {
        id: "chick",
        name: "Chick",
        sheets: &[sheet!("Farm/Chicken/Baby Chicken Yellow.png")],
        cell: 16,
        faces_right: false,
        speed: 18.0,
        walk: anim(0, 0, 4, 8.0, true),
        idle: anim(0, 0, 1, 1.0, true),
        sit: Some(anim(0, 1, 4, 6.0, true)),
        sleep: Some(anim(0, 2, 4, 2.0, true)),
    },
    Species {
        id: "duck",
        name: "Duck",
        sheets: &[sheet!("Farm/Ducks/Duck White.png")],
        cell: 16,
        faces_right: false,
        speed: 20.0,
        walk: anim(0, 0, 4, 8.0, true),
        idle: anim(0, 0, 1, 1.0, true),
        sit: Some(anim(0, 3, 4, 3.0, true)),
        sleep: Some(anim(0, 6, 4, 4.0, false)),
    },
    Species {
        id: "duckling",
        name: "Duckling",
        sheets: &[sheet!("Farm/Ducks/Baby Duck Yellow.png")],
        cell: 16,
        faces_right: false,
        speed: 18.0,
        walk: anim(0, 0, 4, 8.0, true),
        idle: anim(0, 0, 1, 1.0, true),
        sit: Some(anim(0, 1, 4, 6.0, true)),
        sleep: Some(anim(0, 2, 4, 2.0, true)),
    },
    grazer("pig", "Pig", 22.0, &[sheet!("Farm/Pig/Pig Pink.png")]),
    grazer("piglet", "Piglet", 24.0, &[sheet!("Farm/Pig/Baby Pig Pink.png")]),
    grazer("sheep", "Sheep", 20.0, &[sheet!("Farm/Sheep/Sheep Female.png")]),
    grazer("lamb", "Lamb", 22.0, &[sheet!("Farm/Sheep/Baby Sheep .png")]),
    cat("brown-cat", "Brown cat", &[sheet!("Pets/Cats/1/Brown.png")]),
    cat("ginger-cat", "Ginger cat", &[sheet!("Pets/Cats/1/Ginger.png")]),
    cat("caramel-cat", "Caramel cat", &[sheet!("Pets/Cats/1/Light Brown.png")]),
    dog("pup", "Pup", &[sheet!("Pets/Dogs/2/1.png")]),
    dog("spaniel", "Spaniel", &[sheet!("Pets/Dogs/5/1.png")]),
    dog("shiba", "Shiba", &[sheet!("Pets/Dogs/9/3.png")]),
];

pub fn find(id: &str) -> Option<usize> {
    SPECIES.iter().position(|s| s.id == id)
}

// ---------------------------------------------------------------- textures

struct Sheet {
    texture: TextureHandle,
    size: [usize; 2],
    /// Lowest opaque pixel row inside each cell row, per row of the sheet.
    bottoms: Vec<u32>,
}

/// Lazily decoded sprite sheets, keyed by species index.
#[derive(Default)]
pub struct Sprites {
    loaded: HashMap<usize, Vec<Sheet>>,
}

impl Sprites {
    fn sheets(&mut self, ctx: &egui::Context, species: usize) -> &[Sheet] {
        self.loaded.entry(species).or_insert_with(|| {
            let sp = &SPECIES[species];
            sp.sheets
                .iter()
                .enumerate()
                .filter_map(|(i, bytes)| decode(ctx, &format!("pet-{}-{i}", sp.id), bytes, sp.cell))
                .collect()
        })
    }

    /// Draw one frame of `anim` with its feet (bottom-center) at `foot`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        species: usize,
        anim: Anim,
        frame: u32,
        foot: Pos2,
        facing_right: bool,
    ) {
        let sp = &SPECIES[species];
        let Some(sheet) = self.sheets(ctx, species).get(anim.sheet) else { return };
        let cell = sp.cell as f32;
        let (w, h) = (sheet.size[0] as f32, sheet.size[1] as f32);
        let col = frame.min(anim.frames.saturating_sub(1)) as f32;
        let row = anim.row as f32;
        let (u0, u1) = (col * cell / w, (col + 1.0) * cell / w);
        let (v0, v1) = (row * cell / h, (row + 1.0) * cell / h);
        // Flip horizontally by swapping u when facing the other way.
        let flip = facing_right != sp.faces_right;
        let uv = if flip {
            Rect::from_min_max(pos2(u1, v0), pos2(u0, v1))
        } else {
            Rect::from_min_max(pos2(u0, v0), pos2(u1, v1))
        };
        let bottom = sheet.bottoms.get(anim.row as usize).copied().unwrap_or(sp.cell - 1) as f32;
        let size = vec2(cell, cell) * PIXEL;
        let min = pos2(foot.x - size.x / 2.0, foot.y - (bottom + 1.0) * PIXEL);
        let ppp = ctx.pixels_per_point();
        let min = pos2((min.x * ppp).round() / ppp, (min.y * ppp).round() / ppp);
        painter.image(sheet.texture.id(), Rect::from_min_size(min, size), uv, Color32::WHITE);
    }
}

fn decode(ctx: &egui::Context, name: &str, bytes: &[u8], cell: u32) -> Option<Sheet> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    // Feet line per row: the lowest non-transparent pixel within each cell row.
    let bottoms = (0..h / cell)
        .map(|r| {
            (0..cell)
                .rev()
                .find(|&y| (0..w).any(|x| img.get_pixel(x, r * cell + y)[3] > 0))
                .unwrap_or(cell - 1)
        })
        .collect();
    let color = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    let texture = ctx.load_texture(name, color, TextureOptions::NEAREST);
    Some(Sheet {
        texture,
        size: [w as usize, h as usize],
        bottoms,
    })
}

// ---------------------------------------------------------------- behavior

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Walk { target: f32 },
    Idle { until: f64 },
    Sit { until: f64 },
    Sleep { until: f64 },
}

pub struct Pet {
    pub species: usize,
    x: f32,
    facing_right: bool,
    action: Action,
    started: f64,
    last_time: Option<f64>,
    rng: u64,
}

impl Pet {
    pub fn new(species: usize) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15)
            | 1;
        Self {
            species,
            x: f32::NAN,
            facing_right: true,
            action: Action::Idle { until: 0.0 },
            started: 0.0,
            last_time: None,
            rng: seed,
        }
    }

    fn rand(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.rand() as f64
    }

    fn set(&mut self, action: Action, now: f64) {
        self.action = action;
        self.started = now;
    }

    /// Advance the simulation and paint. `area` is the strip it walks along.
    pub fn update_and_paint(&mut self, ctx: &egui::Context, painter: &egui::Painter, sprites: &mut Sprites, area: Rect) {
        let sp = &SPECIES[self.species];
        let now = ctx.input(|i| i.time);
        let dt = self.last_time.map_or(0.0, |t| (now - t).clamp(0.0, 0.1)) as f32;
        self.last_time = Some(now);

        let half = sp.cell as f32 * PIXEL / 2.0;
        let (lo, hi) = (area.left() + half, (area.right() - half).max(area.left() + half));
        if self.x.is_nan() {
            self.x = lo + (hi - lo) * self.rand();
            self.set(Action::Idle { until: now + 1.0 }, now);
        }
        self.x = self.x.clamp(lo, hi);

        match self.action {
            Action::Walk { target } => {
                let target = target.clamp(lo, hi);
                let step = sp.speed * dt;
                if (target - self.x).abs() <= step {
                    self.x = target;
                    let roll = self.rand();
                    let next = if roll < 0.45 && sp.sit.is_some() {
                        Action::Sit { until: now + self.range(4.0, 9.0) }
                    } else if roll < 0.7 && sp.sleep.is_some() {
                        Action::Sleep { until: now + self.range(10.0, 22.0) }
                    } else {
                        Action::Idle { until: now + self.range(1.5, 4.0) }
                    };
                    self.set(next, now);
                } else {
                    self.facing_right = target > self.x;
                    self.x += step * (target - self.x).signum();
                }
            }
            Action::Idle { until } | Action::Sit { until } | Action::Sleep { until } if now >= until => {
                // Wander somewhere at least a little way off.
                let mut target = lo + (hi - lo) * self.rand();
                if (target - self.x).abs() < 60.0 {
                    target = if self.x - lo > hi - self.x { self.x - 120.0 } else { self.x + 120.0 };
                }
                self.set(Action::Walk { target: target.clamp(lo, hi) }, now);
            }
            _ => {}
        }

        let anim = match self.action {
            Action::Walk { .. } => sp.walk,
            Action::Idle { .. } => sp.idle,
            Action::Sit { .. } => sp.sit.unwrap_or(sp.idle),
            Action::Sleep { .. } => sp.sleep.or(sp.sit).unwrap_or(sp.idle),
        };
        let t = (now - self.started).max(0.0);
        let n = (t * anim.fps as f64) as u32;
        let frame = if anim.looped { n % anim.frames.max(1) } else { n.min(anim.frames - 1) };
        sprites.draw(ctx, painter, self.species, anim, frame, pos2(self.x, area.bottom()), self.facing_right);

        // Repaint for the next frame / movement step.
        let mut wait = 1.0 / anim.fps.max(0.5);
        if matches!(self.action, Action::Walk { .. }) {
            wait = wait.min(PIXEL / sp.speed);
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(wait.max(1.0 / 60.0)));
    }
}

/// A still thumbnail for pickers: first walking frame, feet at `foot`.
pub fn thumbnail(ctx: &egui::Context, painter: &egui::Painter, sprites: &mut Sprites, species: usize, foot: Pos2) {
    let sp = &SPECIES[species];
    sprites.draw(ctx, painter, species, sp.walk, 0, foot, true);
}
