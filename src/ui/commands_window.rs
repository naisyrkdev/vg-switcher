//! "Commands Settings": the enable/disable command lists and the
//! status-check service name, each an independently editable field.

use std::cell::Cell;

use native_windows_gui as nwg;

use super::widgets::{self, Placeable, hwnd, label, place_row, text_button};
use super::{BUTTON_H, GAP, LABEL_GAP, LABEL_H, MARGIN, SECTION_GAP, SERVICE_INPUT_H, UNDERLINE_H};
use crate::action::{Action, FieldOp};
use crate::config::{Config, parse_commands};
use crate::error::Result;
use crate::theme::{self, AccentBar, Role};
use crate::ui::resources::Resources;

/// What one field edits: how to show its config value and how to store
/// edited text back.
pub struct FieldSpec {
    pub title: &'static str,
    multiline: bool,
    pub read: fn(&Config) -> String,
    pub write: fn(&mut Config, &str) -> Result<()>,
}

fn read_enable(c: &Config) -> String {
    c.enable_commands.join("\r\n")
}
fn write_enable(c: &mut Config, text: &str) -> Result<()> {
    c.enable_commands = parse_commands(text);
    Ok(())
}
fn read_disable(c: &Config) -> String {
    c.disable_commands.join("\r\n")
}
fn write_disable(c: &mut Config, text: &str) -> Result<()> {
    c.disable_commands = parse_commands(text);
    Ok(())
}
fn read_service(c: &Config) -> String {
    c.status_service.clone()
}
fn write_service(c: &mut Config, text: &str) -> Result<()> {
    c.set_status_service(text)
}

pub static FIELDS: [FieldSpec; 3] = [
    FieldSpec { title: "Enable commands (one per line):", multiline: true, read: read_enable, write: write_enable },
    FieldSpec { title: "Disable commands (one per line):", multiline: true, read: read_disable, write: write_disable },
    FieldSpec { title: "Status-check service name:", multiline: false, read: read_service, write: write_service },
];

const SERVICE_INPUT_W: i32 = 200;

enum Input {
    Multi(nwg::TextBox),
    Single(nwg::TextInput),
}

impl Input {
    fn as_placeable(&self) -> &dyn Placeable {
        match self {
            Input::Multi(b) => b,
            Input::Single(i) => i,
        }
    }
    fn handle(&self) -> &nwg::ControlHandle {
        match self {
            Input::Multi(b) => &b.handle,
            Input::Single(i) => &i.handle,
        }
    }
    fn text(&self) -> String {
        match self {
            Input::Multi(b) => b.text(),
            Input::Single(i) => i.text(),
        }
    }
    fn set_text(&self, text: &str) {
        match self {
            Input::Multi(b) => b.set_text(text),
            Input::Single(i) => i.set_text(text),
        }
    }
    fn set_readonly(&self, readonly: bool) {
        match self {
            Input::Multi(b) => b.set_readonly(readonly),
            Input::Single(i) => i.set_readonly(readonly),
        }
    }
    fn set_focus(&self) {
        match self {
            Input::Multi(b) => b.set_focus(),
            Input::Single(i) => i.set_focus(),
        }
    }
}

/// A label, a read-only input, an accent underline, and the field's own
/// Edit / Restore Default (view mode) or Save / Cancel (edit mode) buttons.
pub struct FieldEditor {
    pub spec: &'static FieldSpec,
    label: nwg::Label,
    input: Input,
    underline: AccentBar,
    edit_btn: nwg::Button,
    restore_btn: nwg::Button,
    save_btn: nwg::Button,
    cancel_btn: nwg::Button,
    editing: Cell<bool>,
}

impl FieldEditor {
    fn build(window: &nwg::Window, spec: &'static FieldSpec, res: &Resources) -> Result<Self> {
        let input = if spec.multiline {
            let mut b = nwg::TextBox::default();
            nwg::TextBox::builder().parent(window).font(Some(&res.mono_font)).readonly(true).build(&mut b)?;
            Input::Multi(b)
        } else {
            let mut i = nwg::TextInput::default();
            nwg::TextInput::builder().parent(window).readonly(true).build(&mut i)?;
            Input::Single(i)
        };
        theme::apply_themed_control(hwnd(input.handle()));

        let editor = FieldEditor {
            spec,
            label: label(window, spec.title, None)?,
            input,
            underline: theme::register_accent_bar(hwnd(&window.handle), Role::Primary),
            edit_btn: text_button(window, "EDIT", Role::Primary)?,
            restore_btn: text_button(window, "RESTORE DEFAULT", Role::Primary)?,
            save_btn: text_button(window, "SAVE", Role::Primary)?,
            cancel_btn: text_button(window, "CANCEL", Role::Primary)?,
            editing: Cell::new(false),
        };
        editor.set_editing(false);
        Ok(editor)
    }

    pub fn is_editing(&self) -> bool {
        self.editing.get()
    }

    pub fn set_editing(&self, editing: bool) {
        self.editing.set(editing);
        self.input.set_readonly(!editing);
        self.edit_btn.set_visible(!editing);
        self.restore_btn.set_visible(!editing);
        self.save_btn.set_visible(editing);
        self.cancel_btn.set_visible(editing);
        if editing {
            self.input.set_focus();
        }
    }

    pub fn text(&self) -> String {
        self.input.text()
    }

    pub fn show(&self, config: &Config) {
        self.input.set_text(&(self.spec.read)(config));
    }

    fn op_for(&self, handle: nwg::ControlHandle) -> Option<FieldOp> {
        [
            (&self.edit_btn, FieldOp::Edit),
            (&self.save_btn, FieldOp::Save),
            (&self.cancel_btn, FieldOp::Cancel),
            (&self.restore_btn, FieldOp::RestoreDefault),
        ]
        .into_iter()
        .find(|(b, _)| b.handle == handle)
        .map(|(_, op)| op)
    }

    /// Lays the field out from `y`; returns the y below it.
    fn layout(&self, mut y: i32, content_w: i32, multiline_h: i32) -> i32 {
        self.label.place(MARGIN, y, content_w, LABEL_H);
        y += LABEL_H + LABEL_GAP;

        let (input_w, input_h) =
            if self.spec.multiline { (content_w, multiline_h) } else { (SERVICE_INPUT_W, SERVICE_INPUT_H) };
        self.input.as_placeable().place(MARGIN, y, input_w, input_h);
        y += input_h;
        theme::set_accent_bar_rect(self.underline, MARGIN, y, input_w, UNDERLINE_H);
        y += UNDERLINE_H + GAP;

        // View-mode and edit-mode buttons share the same spots.
        place_row(MARGIN, y, BUTTON_H, &[(&self.edit_btn, 100), (&self.restore_btn, 150)]);
        place_row(MARGIN, y, BUTTON_H, &[(&self.save_btn, 100), (&self.cancel_btn, 100)]);
        y + BUTTON_H + SECTION_GAP
    }

    /// Fixed (non-input) height of a field, plus its input if single-line.
    fn chrome_h(&self) -> i32 {
        let input = if self.spec.multiline { 0 } else { SERVICE_INPUT_H };
        LABEL_H + LABEL_GAP + input + UNDERLINE_H + GAP + BUTTON_H + SECTION_GAP
    }
}

pub struct CommandsWindow {
    pub window: nwg::Window,
    pub fields: Vec<FieldEditor>,
    close_btn: nwg::Button,
}

impl CommandsWindow {
    pub const MIN_CLIENT: (i32, i32) = (580, 520);

    pub fn build(res: &Resources) -> Result<Self> {
        let window =
            widgets::themed_window("VG Switcher - Commands Settings", (700, 640), (340, 100), true, &res.app_icon)?;
        let fields = FIELDS.iter().map(|spec| FieldEditor::build(&window, spec, res)).collect::<Result<Vec<_>>>()?;
        let close_btn = text_button(&window, "CLOSE", Role::Primary)?;
        let this = CommandsWindow { window, fields, close_btn };
        this.layout();
        Ok(this)
    }

    /// The multi-line boxes share whatever height the fixed parts leave.
    pub fn layout(&self) {
        let (w, h) = self.window.size();
        #[allow(clippy::cast_possible_wrap)]
        let (w, h) = (w as i32, h as i32);
        let content_w = (w - MARGIN * 2).max(200);

        let fixed_h: i32 = self.fields.iter().map(FieldEditor::chrome_h).sum::<i32>() + BUTTON_H + MARGIN * 2;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let multiline_count = self.fields.iter().filter(|f| f.spec.multiline).count().max(1) as i32;
        let box_h = ((h - fixed_h) / multiline_count).max(50);

        let mut y = MARGIN;
        for field in &self.fields {
            y = field.layout(y, content_w, box_h);
        }
        self.close_btn.place((w - MARGIN - 80).max(MARGIN), y, 80, BUTTON_H);
    }

    pub fn action_for(&self, handle: nwg::ControlHandle) -> Option<Action> {
        if handle == self.close_btn.handle {
            return Some(Action::HideCommands);
        }
        self.fields.iter().enumerate().find_map(|(i, f)| f.op_for(handle).map(|op| Action::Field(i, op)))
    }

    /// Shows the saved values in every field that isn't being edited.
    pub fn show_config(&self, config: &Config) {
        for field in self.fields.iter().filter(|f| !f.is_editing()) {
            field.show(config);
        }
    }
}
