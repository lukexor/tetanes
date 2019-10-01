use pix_engine::{event::*, pixel::AlphaMode, *};
use std::time::Duration;

const BEVEL_SIZE: i32 = 2;
const BLOCK_SIZE: i32 = 24;
const TETRO_SIZE: i32 = 4 * BLOCK_SIZE;
const FIELD_WIDTH: i32 = 12;
const FIELD_HEIGHT: i32 = 18;
const FIELD_TOP: i32 = 80;
const FIELD_BOTTOM: i32 = FIELD_TOP + FIELD_HEIGHT * BLOCK_SIZE - BLOCK_SIZE;
const FIELD_LEFT: i32 = 200;
const FIELD_RIGHT: i32 = FIELD_LEFT + FIELD_WIDTH * BLOCK_SIZE - 2 * BLOCK_SIZE;

struct App {
    level: u32,
    score: u32,
    lines: u32,
    paused: bool,
    tetrominos: Vec<Sprite>,
    tetro_field: Sprite,
    current_tetro: Option<Sprite>,
    current_rotation: f32,
    current_x: i32,
    current_y: i32,
}

impl App {
    fn new() -> Self {
        Self {
            level: 1,
            score: 0,
            lines: 0,
            paused: false,
            tetrominos: Vec::new(),
            tetro_field: Sprite::default(),
            current_tetro: Some(Sprite::default()),
            current_rotation: 0.0,
            current_x: 0,
            current_y: 0,
        }
    }

    fn draw_block(x: i32, y: i32, data: &mut StateData) {
        data.fill_rect(x * BLOCK_SIZE, y * BLOCK_SIZE, BLOCK_SIZE, BLOCK_SIZE);
        let c = data.get_draw_color();
        data.set_draw_color(c.highlight());
        data.fill_rect(x * BLOCK_SIZE, y * BLOCK_SIZE, BEVEL_SIZE, BLOCK_SIZE); // Left bevel
        data.fill_rect(x * BLOCK_SIZE, y * BLOCK_SIZE, BLOCK_SIZE, BEVEL_SIZE); // Top bevel
        data.set_draw_color(c.shadow());
        data.fill_rect(
            x * BLOCK_SIZE + BLOCK_SIZE - BEVEL_SIZE,
            y * BLOCK_SIZE,
            BEVEL_SIZE,
            BLOCK_SIZE,
        ); // Right bevel
        data.fill_rect(
            x * BLOCK_SIZE,
            y * BLOCK_SIZE + BLOCK_SIZE - BEVEL_SIZE,
            BLOCK_SIZE,
            BEVEL_SIZE,
        ); // Bottom bevel
        data.set_draw_color(c);
    }

    // TODO Move all tetros to be matrices with 1s and 0s
    // Abstract the draw function to take a matrix, a color, and a rotation
    // Rotate the matrix, then re-draw the blocks
    // Can then use the matrices for collision as well?

    // #
    // # ####
    // #
    // #
    fn tetro_i(rotation: i32, data: &mut StateData) -> Sprite {
        let sprite = Sprite::with_size(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::BLANK);
        data.set_draw_color(pixel::CYAN);
        if rotation % 2 == 0 {
            for y in 0..4 {
                App::draw_block(0, y, data);
            }
        } else {
            for x in 0..4 {
                App::draw_block(x, 1, data);
            }
        }
        data.reset_draw_color();
        data.take_draw_target().unwrap()
    }
    // ##
    // ##
    fn tetro_o(data: &mut StateData) -> Sprite {
        let sprite = Sprite::with_size(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::BLANK);
        data.set_draw_color(pixel::YELLOW);
        for x in 0..2 {
            for y in 0..2 {
                App::draw_block(x, y, data);
            }
        }
        data.reset_draw_color();
        data.take_draw_target().unwrap()
    }
    //  #   #         #
    // ###  ##  ###  ##
    //      #    #    #
    fn tetro_t(data: &mut StateData) -> Sprite {
        let sprite = Sprite::with_size(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::BLANK);
        data.set_draw_color(pixel::MAGENTA);
        App::draw_block(1, 0, data);
        for x in 0..3 {
            App::draw_block(x, 1, data);
        }
        data.reset_draw_color();
        data.take_draw_target().unwrap()
    }
    //  #  #    ##
    //  #  ###  #   ###
    // ##       #     #
    fn tetro_j(data: &mut StateData) -> Sprite {
        let sprite = Sprite::with_size(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::BLANK);
        data.set_draw_color(pixel::BLUE);
        App::draw_block(0, 2, data);
        for y in 0..3 {
            App::draw_block(1, y, data);
        }
        data.reset_draw_color();
        data.take_draw_target().unwrap()
    }
    // #        ##    #
    // #   ###   #  ###
    // ##  #     #
    fn tetro_l(data: &mut StateData) -> Sprite {
        let sprite = Sprite::with_size(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::BLANK);
        data.set_draw_color(pixel::ORANGE);
        App::draw_block(1, 2, data);
        for y in 0..3 {
            App::draw_block(0, y, data);
        }
        data.reset_draw_color();
        data.take_draw_target().unwrap()
    }
    //      #
    //  ##  ##
    // ##    #
    fn tetro_s(data: &mut StateData) -> Sprite {
        let sprite = Sprite::with_size(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::BLANK);
        data.set_draw_color(pixel::GREEN);
        App::draw_block(1, 0, data);
        App::draw_block(2, 0, data);
        App::draw_block(0, 1, data);
        App::draw_block(1, 1, data);
        data.reset_draw_color();
        data.take_draw_target().unwrap()
    }
    //       #
    // ##   ##
    //  ##  #
    fn tetro_z(data: &mut StateData) -> Sprite {
        let sprite = Sprite::with_size(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::BLANK);
        data.set_draw_color(pixel::RED);
        App::draw_block(0, 0, data);
        App::draw_block(1, 0, data);
        App::draw_block(1, 1, data);
        App::draw_block(2, 1, data);
        data.reset_draw_color();
        data.take_draw_target().unwrap()
    }

    fn tetro_field(&mut self, data: &mut StateData) -> Sprite {
        let sprite = Sprite::with_size(BLOCK_SIZE * FIELD_WIDTH, BLOCK_SIZE * FIELD_HEIGHT);
        data.set_draw_target(sprite);
        data.fill(pixel::BLANK);
        data.set_draw_color(pixel::GRAY);
        // Left wall
        for y in 0..FIELD_HEIGHT {
            App::draw_block(0, y, data);
        }
        // Right wall
        for y in 0..FIELD_HEIGHT {
            App::draw_block(FIELD_WIDTH - 1, y, data);
        }
        // Floor
        for x in 0..FIELD_WIDTH {
            App::draw_block(x, FIELD_HEIGHT - 1, data);
        }
        data.reset_draw_color();
        data.take_draw_target().unwrap()
    }

    // (2, 2) -> 10
    // (2, 2) ->
    // 0   ..94, 95
    // 96  ..190, 191
    // 9024..9118, 9119
    // 9120..9214, 9215
    fn rotated(x: i32, y: i32, r: i32) -> i32 {
        match r % 4 {
            0 => y * TETRO_SIZE + x, // 0 degrees
            1 => TETRO_SIZE * TETRO_SIZE - TETRO_SIZE + y - (x * TETRO_SIZE), // 90 degrees
            2 => TETRO_SIZE * TETRO_SIZE - (y * TETRO_SIZE) - x, // 180 degrees
            3 => TETRO_SIZE - 1 - y + (x * TETRO_SIZE), // 270 degrees
            _ => panic!("impossible"),
        }
    }

    fn valid_move(&mut self, tetro_id: usize, rotation: i32, x: i32, y: i32) -> bool {
        for tx in 0..TETRO_SIZE {
            for ty in 0..TETRO_SIZE {
                // Index into piece
                let ti = App::rotated(tx, ty, rotation);

                // Index into field
                let fi = (y + ty) * BLOCK_SIZE * FIELD_WIDTH + (x + tx);

                if x + tx >= 0 && x + tx < BLOCK_SIZE * FIELD_WIDTH {
                    if y + ty >= 0 && y + ty < BLOCK_SIZE * FIELD_HEIGHT {
                        if self.tetrominos[tetro_id].as_pixels()[ti as usize] != pixel::BLANK
                            && self.tetro_field.as_pixels()[fi as usize] != pixel::BLANK
                        {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    fn draw_field(&mut self, data: &mut StateData) {
        data.draw_sprite(200, 80, &self.tetro_field);
        data.draw_string(545, 100, "SCORE".into());
        data.draw_string(545, 132, format!("{}", self.score));
        data.draw_string(545, 200, "LEVEL".into());
        data.draw_string(545, 232, format!("{}", self.level));
        data.draw_string(545, 300, "LINES".into());
        data.draw_string(545, 332, format!("{}", self.lines));
    }
}

impl State for App {
    fn on_start(&mut self, data: &mut StateData) -> bool {
        self.tetrominos.push(App::tetro_i(0, data));
        self.tetrominos.push(App::tetro_o(data));
        self.tetrominos.push(App::tetro_t(data));
        self.tetrominos.push(App::tetro_j(data));
        self.tetrominos.push(App::tetro_l(data));
        self.tetrominos.push(App::tetro_s(data));
        self.tetrominos.push(App::tetro_z(data));
        self.tetro_field = self.tetro_field(data);
        data.set_font_scale(3);
        self.current_y = FIELD_TOP;
        self.current_x = FIELD_LEFT + (FIELD_RIGHT - FIELD_LEFT) / 2;
        self.current_tetro = Some(self.tetrominos[rand::random::<usize>() % 7].clone());
        true
    }
    fn on_update(&mut self, elapsed: Duration, data: &mut StateData) -> bool {
        data.clear(pixel::BLACK);
        self.draw_field(data);
        data.set_alpha_mode(AlphaMode::Mask);

        let elapsed = elapsed.as_secs_f32();

        if data.get_key(Key::Z).held {
            self.current_rotation -= 2.0 * elapsed;
        }
        if data.get_key(Key::X).held {
            self.current_rotation += 2.0 * elapsed;
        }
        if data.get_key(Key::Right).held && self.current_x < (FIELD_RIGHT - BLOCK_SIZE) {
            self.current_x += 4 + (10.0 * elapsed).ceil() as i32;
        } else if data.get_key(Key::Left).held && self.current_x > (FIELD_LEFT + BLOCK_SIZE) {
            self.current_x -= 4 + (10.0 * elapsed).ceil() as i32;
        }
        if data.get_key(Key::Down).held && self.current_y < (FIELD_BOTTOM - BLOCK_SIZE) {
            self.current_y += 2 * BLOCK_SIZE;
        }
        data.draw_sprite(
            self.current_x,
            self.current_y,
            self.current_tetro.as_ref().unwrap(),
        );

        data.set_alpha_mode(AlphaMode::Normal);
        true
    }
    fn on_stop(&mut self, _data: &mut StateData) -> bool {
        true
    }
}

pub fn main() {
    let app = App::new();
    let mut engine = PixEngine::new("Tetris", app, 800, 600);
    engine.run().unwrap();
}
