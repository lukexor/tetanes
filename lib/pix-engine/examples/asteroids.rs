use pix_engine::event::*;
use pix_engine::*;
use std::f64::consts;
use std::time::Duration;

const SHIP_SCALE: f32 = 4.0;
const ASTEROID_SIZE: u32 = 64;
const MIN_ASTEROID_SIZE: u32 = 16;
const SHIP_THRUST: f32 = 150.0;
const MAX_ASTEROID_SPEED: f32 = 50.0;
const SHATTERED_ASTEROID_SPEED: f32 = 80.0;
const BULLET_SPEED: f32 = 200.0;
const ASTEROID_SAFE_RADIUS: f32 = 80.0; // So asteroids don't spawn near player
const PI: f32 = consts::PI as f32;

struct App {
    asteroids: Vec<SpaceObj>,
    bullets: Vec<SpaceObj>,
    ship: SpaceObj,
    level: u32,
    lives: u32,
    score: i32,
    exploded: bool,
    ship_model: Vec<(f32, f32)>,
    asteroid_model: Vec<(f32, f32)>,
    paused: bool,
}

#[derive(Default)]
struct SpaceObj {
    size: u32,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    angle: f32,
    destroyed: bool,
}

impl SpaceObj {
    fn new(size: u32, x: f32, y: f32, dx: f32, dy: f32, angle: f32) -> Self {
        Self {
            size,
            x,
            y,
            dx,
            dy,
            angle,
            destroyed: false,
        }
    }
    fn rand_asteroid(ship: &SpaceObj, data: &StateData) -> Self {
        let mut x = rand::random::<f32>() * data.screen_width() as f32;
        if x > (ship.x - ASTEROID_SAFE_RADIUS) && x < (ship.x + ASTEROID_SAFE_RADIUS) {
            let diff = ASTEROID_SAFE_RADIUS - (ship.x - x).abs();
            if ship.x > x {
                x -= diff;
            } else {
                x += diff;
            }
        }
        let mut y = rand::random::<f32>() * data.screen_height() as f32;
        if y > (ship.y - ASTEROID_SAFE_RADIUS) && y < (ship.y + ASTEROID_SAFE_RADIUS) {
            let diff = ASTEROID_SAFE_RADIUS - (ship.y - y).abs();
            if ship.y > y {
                y -= diff;
            } else {
                y += diff;
            }
        }

        Self {
            size: ASTEROID_SIZE,
            x,
            y,
            dx: (rand::random::<f32>() - 0.5) * 2.0 * MAX_ASTEROID_SPEED,
            dy: (rand::random::<f32>() - 0.5) * 2.0 * MAX_ASTEROID_SPEED,
            angle: rand::random::<f32>() * 360.0,
            destroyed: false,
        }
    }
}

impl App {
    fn new() -> Self {
        Self {
            asteroids: Vec::new(),
            bullets: Vec::new(),
            ship: SpaceObj::default(),
            level: 1,
            lives: 4,
            score: 0,
            exploded: false,
            ship_model: Vec::new(),
            asteroid_model: Vec::new(),
            paused: false,
        }
    }

    fn spawn_new_ship(&mut self, data: &StateData) {
        self.ship.x = data.screen_width() as f32 / 2.0;
        self.ship.y = data.screen_height() as f32 / 2.0;
        self.ship.dx = 0.0;
        self.ship.dy = 0.0;
        self.ship.angle = 0.0;

        let asteroid_count = if self.asteroids.len() > 0 {
            std::cmp::min(self.level + 2, self.asteroids.len() as u32)
        } else {
            self.level + 2
        };
        self.asteroids.clear();
        self.bullets.clear();
        for _ in 0..asteroid_count {
            self.asteroids
                .push(SpaceObj::rand_asteroid(&self.ship, data));
        }
    }

    fn exploded(&mut self, data: &StateData) {
        self.lives -= 1;
        self.score -= 500;
        self.exploded = false;
        self.spawn_new_ship(data);
    }

    fn reset(&mut self, data: &StateData) {
        self.spawn_new_ship(data);
        self.level = 1;
        self.lives = 4;
        self.score = 0;
        self.exploded = false;
    }
}

impl State for App {
    fn on_start(&mut self, data: &mut StateData) -> PixEngineResult<()> {
        data.enable_coord_wrapping(true);
        self.ship_model = vec![(0.0, -5.0), (-2.5, 2.5), (2.5, 2.5)];
        for i in 0..20 {
            let noise = rand::random::<f32>() * 0.4 + 0.8;
            let a = (i as f32 / 20.0) * 2.0 * PI;
            let x = noise * a.sin();
            let y = noise * a.cos();
            self.asteroid_model.push((x, y));
        }
        self.spawn_new_ship(data);
        Ok(())
    }

    fn on_update(&mut self, elapsed: Duration, data: &mut StateData) -> PixEngineResult<()> {
        let elapsed = elapsed.as_secs_f32();

        if data.get_key(Key::Escape).pressed {
            self.paused = !self.paused;
        }
        if data.get_key(Key::R).pressed {
            self.reset(data);
        }
        if self.paused {
            return Ok(());
        }

        data.fill(pixel::BLACK);
        // data.clear();

        if self.exploded {
            if self.lives > 0 {
                self.exploded(data);
            } else {
                data.set_font_scale(3);
                data.draw_string(
                    data.screen_width() / 2 - 108,
                    data.screen_height() / 3 - 24,
                    "GAME OVER",
                    pixel::WHITE,
                );
                data.set_font_scale(1);
                data.draw_string(
                    data.screen_width() / 2 - 88,
                    data.screen_height() / 3 + 16,
                    "PRESS SPACE TO RESTART",
                    pixel::WHITE,
                );
                data.set_font_scale(2);
                if data.get_key(Key::Space).pressed {
                    self.reset(data);
                }
            }
            return Ok(());
        }

        // Draw Level, Lives, & Score
        data.draw_string(
            4,
            4,
            &format!("LEVEL: {}  SCORE: {}", self.level, self.score),
            pixel::WHITE,
        );
        for i in 0..self.lives {
            data.draw_wireframe(
                &self.ship_model,
                12.0 + (i as f32 * 14.0),
                36.0,
                0.0,
                2.0,
                pixel::WHITE,
            );
        }

        // Steer
        if data.get_key(Key::Left).held {
            self.ship.angle -= 5.0 * elapsed;
        } else if data.get_key(Key::Right).held {
            self.ship.angle += 5.0 * elapsed;
        }

        // Thrust
        if data.get_key(Key::Up).held {
            self.ship.dx += self.ship.angle.sin() * SHIP_THRUST * elapsed;
            self.ship.dy += -self.ship.angle.cos() * SHIP_THRUST * elapsed;
        }

        self.ship.x += self.ship.dx * elapsed;
        self.ship.y += self.ship.dy * elapsed;

        // Keep ship in game space
        data.wrap_coords(self.ship.x, self.ship.y, &mut self.ship.x, &mut self.ship.y);

        // Shoot a bullet
        if data.get_key(Key::Space).released {
            self.bullets.push(SpaceObj::new(
                0,
                self.ship.x,
                self.ship.y,
                BULLET_SPEED * self.ship.angle.sin(),
                BULLET_SPEED * -self.ship.angle.cos(),
                100.0,
            ));
        }

        // Draw asteroids
        for a in self.asteroids.iter_mut() {
            // Ship collision
            if data.is_inside_circle(a.x, a.y, a.size as f32, self.ship.x, self.ship.y) {
                self.exploded = true;
            }

            a.x += a.dx * elapsed;
            a.y += a.dy * elapsed;
            a.angle += 0.5 * elapsed; // Give some twirl
            data.wrap_coords(a.x, a.y, &mut a.x, &mut a.y);
            data.draw_wireframe(
                &self.asteroid_model,
                a.x,
                a.y,
                a.angle,
                a.size as f32,
                pixel::YELLOW,
            );
        }

        let mut new_asteroids = Vec::new();
        // Draw bullets
        for b in self.bullets.iter_mut() {
            b.x += b.dx * elapsed;
            b.y += b.dy * elapsed;
            b.angle -= 1.0 * elapsed;

            for a in self.asteroids.iter_mut() {
                if data.is_inside_circle(a.x, a.y, a.size as f32, b.x, b.y) {
                    // Asteroid hit
                    b.destroyed = true; // Removes bullet

                    if a.size > MIN_ASTEROID_SIZE {
                        // Break into two
                        let a1 = rand::random::<f32>() * 2.0 * PI;
                        let a2 = rand::random::<f32>() * 2.0 * PI;
                        new_asteroids.push(SpaceObj::new(
                            a.size >> 1,
                            a.x,
                            a.y,
                            SHATTERED_ASTEROID_SPEED * a1.sin(),
                            SHATTERED_ASTEROID_SPEED * a1.cos(),
                            0.0,
                        ));
                        new_asteroids.push(SpaceObj::new(
                            a.size >> 1,
                            a.x,
                            a.y,
                            SHATTERED_ASTEROID_SPEED * a2.sin(),
                            SHATTERED_ASTEROID_SPEED * a2.cos(),
                            0.0,
                        ));
                    }
                    a.destroyed = true; // Remove asteroid
                    self.score += 100;
                }
            }
        }
        self.asteroids.append(&mut new_asteroids);

        // Remove offscreen/destroyed bullets
        self.bullets.retain(|b| {
            !b.destroyed
                && b.x >= 1.0
                && b.x < data.screen_width() as f32
                && b.y >= 1.0
                && b.y < data.screen_height() as f32
        });
        // Remove destroyed asteroids
        self.asteroids.retain(|a| !a.destroyed);

        // Draw bullets
        for b in self.bullets.iter() {
            data.fill_circle(b.x as u32, b.y as u32, 1, pixel::WHITE);
        }

        // Draw ship
        data.draw_wireframe(
            &self.ship_model,
            self.ship.x,
            self.ship.y,
            self.ship.angle,
            SHIP_SCALE,
            pixel::WHITE,
        );

        // Win level
        if self.asteroids.is_empty() {
            self.level += 1;
            self.score += 1000;
            self.bullets.clear();
            for _ in 0..(self.level + 2) {
                self.asteroids
                    .push(SpaceObj::rand_asteroid(&self.ship, data));
            }
        }

        Ok(())
    }
}

pub fn main() {
    let app = App::new();
    let mut engine = PixEngine::new("Asteroids", app, 800, 600);
    if let Err(e) = engine.run() {
        eprintln!("Encountered a PixEngineErr: {}", e.to_string());
    }
}
