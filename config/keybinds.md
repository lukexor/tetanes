# Keybinds JSON

## Keyboard Mappings

A `keys` array with the following values:

- `key`: String that maps to a `pix_engine::prelude::Key` variant.
- `keymod`: Number that maps to a `pix_engine::prelude::KeyMod` constant:
  - `NONE`: `0`
  - `SHIFT`: `1`,
  - `CTRL`: `64`,
  - `ALT`: `256`,
  - `GUI`: `1024`,
- `action`: Object that maps to an `nes::Action` variant. e.g.
  `{ "Gamepad": "Left" } }`

## Controller Button Mappings

A `buttons` array with the following values:

- `controller`: `One`, `Two`, `Three`, or `Four` for each possible player.
- `button`: String that maps to a `pix_engine::prelude::ControllerButton`
  variant.
- `action`: Object that maps to an `nes::Action` variant. e.g.
  `{ "Nes": ToggleMenu" } }`

## Controller Axis Mappings

A `axes` array with the following values:

- `controller`: `One`, `Two`, `Three`, or `Four` for each possible player.
- `axis`: String that maps to a `pix_engine::prelude::Axis` variant.
- `direction`: `None`, `Positive`, or `Negative` to indicate axis direction.
- `action`: Object that maps to an `nes::Action` variant. e.g.
  `{ "ZeroAxis": ["Left", "Right"] } }`
