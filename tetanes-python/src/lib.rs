use numpy::PyArray3;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::io::Cursor;
use tetanes_core::input::JoypadBtnState;
use tetanes_core::mem::Read;
use tetanes_core::prelude::*;

/// NES Emulator Environment for Reinforcement Learning
#[pyclass]
pub struct NesEnv {
    control_deck: ControlDeck,
    rom_loaded: bool,
}

#[pymethods]
#[allow(non_local_definitions)]
impl NesEnv {
    #[new]
    #[pyo3(signature = (headless = false))]
    fn new(headless: bool) -> Self {
        let mut config = Config::default();

        if headless {
            config.headless_mode = HeadlessMode::NO_AUDIO | HeadlessMode::NO_VIDEO;
        }

        let control_deck = ControlDeck::with_config(config);

        Self {
            control_deck,
            rom_loaded: false,
        }
    }

    /// Load a ROM from bytes
    fn load_rom(&mut self, rom_name: String, rom_data: &PyBytes) -> PyResult<()> {
        let mut cursor = Cursor::new(rom_data.as_bytes());
        self.control_deck
            .load_rom(rom_name, &mut cursor)
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to load ROM: {e}"
                ))
            })?;

        self.rom_loaded = true;
        self.reset()?;
        Ok(())
    }

    /// Reset the environment to initial state
    fn reset(&mut self) -> PyResult<()> {
        if !self.rom_loaded {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "No ROM loaded",
            ));
        }

        self.control_deck.reset(ResetKind::Hard);
        Ok(())
    }

    /// Step the environment with given actions
    /// Actions: [player1_a, player1_b, player1_select, player1_start, player1_up, player1_down, player1_left, player1_right]
    #[pyo3(signature = (actions, render = true))]
    fn step(
        &mut self,
        actions: Vec<bool>,
        render: bool,
    ) -> PyResult<(PyObject, f64, bool, bool, PyObject)> {
        if !self.rom_loaded {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "No ROM loaded",
            ));
        }

        // Validate action vector length
        if actions.len() != 8 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Expected 8 actions, got {}",
                actions.len()
            )));
        }

        // Apply input actions
        self.apply_actions(&actions);

        // Step one frame
        let cycles = self.control_deck.clock_frame().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Emulation error: {e}"))
        })?;

        // Get observation
        let observation = if render {
            self.get_observation()?
        } else {
            Python::with_gil(|py| py.None())
        };

        // Placeholder values - SMB2 gym doesn't use these
        let reward = 0.0;
        let terminated = false;
        let truncated = false;

        // Info dict with cycle count
        let info = Python::with_gil(|py| {
            let info_dict = pyo3::types::PyDict::new(py);
            let _ = info_dict.set_item("cycles", cycles);
            info_dict.to_object(py)
        });

        Ok((observation, reward, terminated, truncated, info))
    }

    /// Get current frame as RGB array
    fn get_observation(&mut self) -> PyResult<PyObject> {
        let frame_buffer = self.control_deck.frame_buffer();

        Python::with_gil(|py| {
            // Create 3D array directly without intermediate allocation
            let mut reshaped = vec![vec![vec![0u8; 3]; 256]; 240];

            // Convert RGBA to RGB and reshape in one pass
            for (i, chunk) in frame_buffer.chunks_exact(4).enumerate() {
                let y = i / 256;
                let x = i % 256;
                if y < 240 {
                    reshaped[y][x][0] = chunk[0]; // R
                    reshaped[y][x][1] = chunk[1]; // G
                    reshaped[y][x][2] = chunk[2]; // B
                }
            }

            let array = PyArray3::<u8>::from_vec3(py, &reshaped)?;
            Ok(array.to_object(py))
        })
    }

    /// Save state to slot
    fn save_state(&mut self, slot: u8) -> PyResult<()> {
        let path = format!("save_state_{slot}.sav");
        self.control_deck.save_state(&path).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to save state: {e}"))
        })
    }

    /// Load state from slot
    fn load_state(&mut self, slot: u8) -> PyResult<()> {
        let path = format!("save_state_{slot}.sav");
        self.control_deck.load_state(&path).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to load state: {e}"))
        })
    }

    /// Save state to a specific file path
    fn save_state_to_path(&mut self, path: &str) -> PyResult<()> {
        self.control_deck.save_state(path).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to save state to path '{path}': {e}"
            ))
        })
    }

    /// Load state from a specific file path
    fn load_state_from_path(&mut self, path: &str) -> PyResult<()> {
        self.control_deck.load_state(path).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to load state from path '{path}': {e}"
            ))
        })
    }

    /// Read a single byte from RAM
    fn read_ram(&self, address: u16) -> PyResult<u8> {
        if address >= 0x800 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "RAM address {address:#x} out of bounds (must be < 0x800)"
            )));
        }

        Ok(self.control_deck.cpu().peek(address))
    }
}

impl NesEnv {
    fn apply_actions(&mut self, actions: &[bool]) {
        debug_assert_eq!(
            actions.len(),
            8,
            "Actions should be validated before calling apply_actions"
        );

        let joypad = self.control_deck.joypad_mut(Player::One);

        // Map boolean actions to joypad buttons using const array for button states
        const BUTTON_STATES: [JoypadBtnState; 8] = [
            JoypadBtnState::A,
            JoypadBtnState::B,
            JoypadBtnState::SELECT,
            JoypadBtnState::START,
            JoypadBtnState::UP,
            JoypadBtnState::DOWN,
            JoypadBtnState::LEFT,
            JoypadBtnState::RIGHT,
        ];

        for (button, &pressed) in BUTTON_STATES.iter().zip(actions.iter()) {
            joypad.set_button(*button, pressed);
        }
    }
}

/// Python module for TetaNES RL environment
#[pymodule]
fn _tetanes(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<NesEnv>()?;
    Ok(())
}

