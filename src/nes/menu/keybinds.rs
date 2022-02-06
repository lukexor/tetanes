use crate::nes::{menu::Menu, Mode, Nes};
use anyhow::anyhow;
use pix_engine::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Player {
    One,
    Two,
    Three,
    Four,
}

impl AsRef<str> for Player {
    fn as_ref(&self) -> &str {
        match self {
            Self::One => "Player One",
            Self::Two => "Player Two",
            Self::Three => "Player Three",
            Self::Four => "Player Four",
        }
    }
}

impl TryFrom<usize> for Player {
    type Error = PixError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::One),
            1 => Ok(Self::Two),
            2 => Ok(Self::Three),
            3 => Ok(Self::Four),
            _ => Err(anyhow!("invalid `Player`").into()),
        }
    }
}

impl Nes {
    pub(super) fn render_keybinds(
        &mut self,
        s: &mut PixState,
        menu: Menu,
        player: Player,
    ) -> PixResult<()> {
        let mut selected = player as usize;
        s.next_width(200);
        if s.select_box(
            "",
            &mut selected,
            &[Player::One, Player::Two, Player::Three, Player::Four],
            4,
        )? {
            self.mode = Mode::InMenu(menu, selected.try_into()?);
        }
        s.spacing()?;

        self.render_gamepad_binds(player, s)?;
        self.render_emulator_binds(player, s)?;
        self.render_debugger_binds(player, s)?;
        Ok(())
    }

    fn render_gamepad_binds(&mut self, _player: Player, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("Gamepad", |s: &mut PixState| {
            s.text("Coming soon!")?;
            // let mut bindings: Vec<(&Action, &Input)> = self
            //     .config
            //     .input_bindings
            //     .iter()
            //     .filter(|(_, action)| matches!(action, Action::Gamepad(..) | Action::ZeroAxis(..)))
            //     .map(|(binding, action)| (action, binding))
            //     .collect();
            // bindings.sort_by_key(|(action, _)| *action);
            // for (action, binding) in bindings {
            //     // Keyboard bindings are for Player One only
            //     if matches!(binding, Input::Key(..)) && player != Player::One {
            //         continue;
            //     }
            //     match action {
            //         Action::Gamepad(btn) => {
            //             // dbg!(binding, action);
            //             s.text(btn)?;
            //             s.same_line(None);
            //             s.text(":")?;
            //             s.same_line(None);
            //             s.text(binding.to_string())?;
            //         }
            //         Action::ZeroAxis(..) => {}
            //         _ => (),
            //     }
            // }
            s.spacing()?;
            Ok(())
        })?;
        Ok(())
    }

    fn render_emulator_binds(&mut self, _player: Player, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("Emulator", |s: &mut PixState| {
            s.text("Coming soon!")?;
            // Action::Nes
            // Action::Menu
            // Action::Feature
            // Action::Setting
            s.spacing()?;
            Ok(())
        })?;
        Ok(())
    }

    fn render_debugger_binds(&mut self, _player: Player, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("Debugger", |s: &mut PixState| {
            s.text("Coming soon!")?;
            // Action::Debug
            s.spacing()?;
            Ok(())
        })?;
        Ok(())
    }
}
