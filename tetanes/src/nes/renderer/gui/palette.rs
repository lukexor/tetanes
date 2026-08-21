//! Colors the Debugger paints with, named for what they mark.
//!
//! Every one resolves from the [`Visuals`] in force, so the Debugger tracks whatever theme the
//! rest of the UI is drawn in.

use egui::{Color32, Stroke, Visuals};

/// The Debugger's colors, resolved from a theme.
#[derive(Debug, Copy, Clone, PartialEq)]
#[must_use]
pub struct Palette {
    /// The address a disassembly row starts at.
    pub address: Color32,
    /// The instruction's bytes, opcode first.
    pub bytes: Color32,
    /// The mnemonic of an instruction the 6502 documents.
    pub mnemonic: Color32,
    /// The mnemonic of one it does not, which the `*` in the same column also marks.
    pub mnemonic_unofficial: Color32,
    /// The operand as written.
    pub operand: Color32,
    /// The address the operand lands on once a register is added.
    pub effective: Color32,
    /// What sits at the address the operand reaches.
    pub resolved: Color32,
    /// A collapsed range of addresses that were not disassembled.
    pub block: Color32,
    /// Behind the row the console is about to execute.
    pub pc_background: Color32,
    /// The text of that row.
    pub pc_text: Color32,
    /// Around the row the user last clicked.
    pub selection: Stroke,
    /// A breakpoint watching execution, the strongest access a gutter mark shows.
    pub breakpoint_exec: Color32,
    /// One watching writes.
    pub breakpoint_write: Color32,
    /// One watching reads.
    pub breakpoint_read: Color32,
    /// One listed without being armed.
    pub breakpoint_disabled: Color32,
    /// A name given to an address, on its own line above the row it names.
    pub label: Color32,
    /// A note written about a row, after the instruction.
    pub comment: Color32,
    /// A byte the memory pane can type over.
    pub memory_writable: Color32,
    /// One it cannot: ROM, an unmapped page, or a register.
    pub memory_readonly: Color32,
    /// Behind the gutter column, separating it from the disassembly.
    pub gutter: Color32,
    /// Behind it while the pointer is over a row's share of it.
    pub gutter_hovered: Color32,
}

impl Palette {
    /// Resolve the palette from the theme in force.
    pub fn new(visuals: &Visuals) -> Self {
        let text = visuals.text_color();
        let weak = visuals.weak_text_color();
        Self {
            address: weak,
            // Dimmer than the address, which at least indexes the row. Raw bytes are there to be
            // read when something is wrong, not on the way past.
            bytes: visuals.gray_out(weak),
            mnemonic: visuals.strong_text_color(),
            // The same yellow the UI warns in. An unofficial opcode is one the hardware runs and
            // no datasheet promises.
            mnemonic_unofficial: visuals.warn_fg_color,
            operand: text,
            // The link color, since the effective address points at somewhere else the way a
            // link does.
            effective: visuals.hyperlink_color,
            resolved: weak,
            block: visuals.gray_out(text),
            // PC and selection both mark "here", so both come from the theme's selection colors.
            // A fill for PC and an outline for selection keeps them apart on the same row.
            pc_background: visuals.selection.bg_fill,
            pc_text: visuals.selection.stroke.color,
            selection: visuals.selection.stroke,
            // Red, green and blue by access, the way Mesen colors them, so a glance at the
            // gutter says what a mark is watching.
            breakpoint_exec: visuals.error_fg_color,
            breakpoint_write: Color32::from_rgb(0x4C, 0xAF, 0x50),
            breakpoint_read: Color32::from_rgb(0x42, 0xA5, 0xF5),
            breakpoint_disabled: visuals.gray_out(visuals.error_fg_color),
            // A name is the reader's own word for the address, so it takes the accent the theme
            // points at things with. A comment sits behind the code it annotates.
            label: visuals.hyperlink_color,
            comment: visuals.gray_out(visuals.text_color()),
            // Full strength for a byte a click can change, dimmed for one it cannot.
            memory_writable: visuals.strong_text_color(),
            memory_readonly: visuals.gray_out(text),
            // Recessed, the way the theme sinks a text box, so the column reads as its own
            // strip rather than as margin.
            gutter: visuals.extreme_bg_color,
            // A row whose gutter has no breakpoint paints nothing, so hover is the only sign the
            // column can be clicked at all.
            gutter_hovered: visuals.widgets.hovered.bg_fill,
        }
    }
}
