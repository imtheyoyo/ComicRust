//! Clavier, glisser-déposer, plein écran et titre de la fenêtre.

use std::path::PathBuf;

use eframe::egui::{Context, Key, ViewportCommand};

use crate::app::ComicRustApp;
use crate::settings::FitMode;

impl ComicRustApp {
    pub(crate) fn handle_input(&mut self, ctx: &Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect()
        });
        if let Some(p) = dropped.into_iter().next() {
            self.open_path(ctx, p);
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if self.mode == crate::app::Mode::Library {
            // Vue Bibliothèque : seuls Ctrl+O et le plein écran.
            let (open, full) = ctx.input(|i| (i.modifiers.command && i.key_pressed(Key::O), i.key_pressed(Key::F11)));
            if open {
                self.open_dialog(ctx);
            }
            if full {
                self.toggle_fullscreen(ctx);
            }
            return;
        }
        let manga = self.settings.manga;
        let (mut next, mut prev) = (false, false);
        let mut actions: Vec<Key> = Vec::new();
        ctx.input(|i| {
            for k in [
                Key::ArrowRight,
                Key::ArrowLeft,
                Key::Space,
                Key::Backspace,
                Key::PageDown,
                Key::PageUp,
                Key::Home,
                Key::End,
                Key::F11,
                Key::Escape,
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::D,
                Key::M,
                Key::T,
                Key::I,
                Key::L,
                Key::F,
                Key::Plus,
                Key::Equals,
                Key::Minus,
                Key::Num0,
                Key::O,
            ] {
                if i.key_pressed(k) {
                    actions.push(k);
                }
            }
            if i.modifiers.command && i.key_pressed(Key::O) {
                actions.push(Key::F1); // marqueur "ouvrir"
            }
            if i.modifiers.command && i.key_pressed(Key::W) {
                actions.push(Key::F2); // marqueur "fermer le livre"
            }
        });
        for k in actions {
            match k {
                Key::ArrowRight => {
                    if manga {
                        prev = true
                    } else {
                        next = true
                    }
                }
                Key::ArrowLeft => {
                    if manga {
                        next = true
                    } else {
                        prev = true
                    }
                }
                Key::Space | Key::PageDown => next = true,
                Key::Backspace | Key::PageUp => prev = true,
                Key::Home => self.go_to(0),
                Key::End => self.go_to(usize::MAX),
                Key::F11 | Key::F => self.toggle_fullscreen(ctx),
                Key::Escape if self.fullscreen => self.toggle_fullscreen(ctx),
                Key::Num1 => self.settings.fit = FitMode::Page,
                Key::Num2 => self.settings.fit = FitMode::Width,
                Key::Num3 => self.settings.fit = FitMode::Height,
                Key::Num4 => self.settings.fit = FitMode::Original,
                Key::D => self.settings.double_page ^= true,
                Key::M => self.settings.manga ^= true,
                Key::T => self.settings.show_thumbnails ^= true,
                Key::I => self.show_info ^= true,
                Key::L => self.show_list_panel ^= true,
                Key::Plus | Key::Equals => self.zoom = (self.zoom * 1.25).min(8.0),
                Key::Minus => self.zoom = (self.zoom / 1.25).max(0.1),
                Key::Num0 => self.zoom = 1.0,
                Key::F2 => {
                    self.close_book(ctx);
                    return;
                }
                Key::F1 => self.open_dialog(ctx),
                _ => {}
            }
        }
        if next {
            self.next_page(ctx);
        }
        if prev {
            self.prev_page(ctx);
        }
    }

    pub(crate) fn toggle_fullscreen(&mut self, ctx: &Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }

    pub(crate) fn update_title(&mut self, ctx: &Context) {
        let title = match &self.book {
            Some(b) if b.opened => format!("{} — ComicRust", b.title),
            Some(b) => format!("{} (ouverture…) — ComicRust", b.title),
            None => "ComicRust".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}
