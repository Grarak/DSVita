use crate::emu::input::Keycode;
use crate::presenter::{PresentEvent, Presenter};
use std::cmp::min;
use std::rc::Rc;

pub struct MenuPresenter<'a> {
    presenter: &'a mut Presenter,
    pressed: bool,
    root_menu: Menu<'a>,
    menu_stack: Vec<usize>,
}

impl<'a> MenuPresenter<'a> {
    pub fn new(presenter: &'a mut Presenter, root_menu: Menu<'a>) -> Self {
        MenuPresenter {
            presenter,
            pressed: false,
            root_menu,
            menu_stack: Vec::new(),
        }
    }

    pub fn present(&mut self) {
        while let PresentEvent::Inputs { keymap, .. } = self.presenter.poll_event() {
            let mut menu = &mut self.root_menu;
            for i in &self.menu_stack {
                menu = &mut menu.entries[*i];
            }

            let pressed = !keymap;
            let keys_pressed = if self.pressed { 0 } else { pressed };
            self.pressed = pressed != 0;
            self.presenter.present_menu(menu);

            if keys_pressed & (1 << Keycode::A as u8) != 0 && !menu.entries.is_empty() {
                menu.entries[menu.selected].selected = 0;
                let on_select = menu.entries[menu.selected].on_select.clone();
                match on_select(&mut menu.entries[menu.selected]) {
                    MenuAction::Refresh => {
                        let on_select = menu.on_select.clone();
                        on_select(menu);
                    }
                    MenuAction::EnterSubMenu => self.menu_stack.push(menu.selected),
                    MenuAction::Quit => break,
                }
            } else if keys_pressed & (1 << Keycode::B as u8) != 0 {
                self.menu_stack.pop();
            } else if keys_pressed & (1 << Keycode::Up as u8) != 0 {
                menu.selected = min(menu.selected.wrapping_sub(1), menu.entries.len() - 1);
            } else if keys_pressed & (1 << Keycode::Down as u8) != 0 && !menu.entries.is_empty() {
                menu.selected = (menu.selected + 1) % menu.entries.len();
            }

            self.presenter.wait_vsync();
        }
    }
}

pub struct Menu<'a> {
    pub title: String,
    pub entries: Vec<Menu<'a>>,
    pub selected: usize,
    on_select: Rc<dyn Fn(&mut Menu<'a>) -> MenuAction + 'a>,
}

impl<'a> Menu<'a> {
    pub fn new<F: Fn(&mut Menu<'a>) -> MenuAction + 'a>(title: impl Into<String>, entries: Vec<Menu<'a>>, on_select: F) -> Self {
        Menu {
            title: title.into(),
            entries,
            selected: 0,
            on_select: Rc::new(on_select),
        }
    }
}

pub enum MenuAction {
    Refresh,
    EnterSubMenu,
    Quit,
}
