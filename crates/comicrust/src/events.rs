//! Événements venus des fils d'arrière-plan : liste de lecture chargée, pages et vignettes décodées.

use eframe::egui::{Context, TextureOptions};

use crate::app::ComicRustApp;
use crate::book::loader::Event;
use crate::book::texture_options;
use comicrust_core::ComicInfo;

use crate::library::watch::WatchEvent;

impl ComicRustApp {
    /// Modifications faites dans ComicRack et listes relues.
    fn poll_watch(&mut self) {
        let events: Vec<WatchEvent> =
            self.library.watcher.as_ref().map(|w| w.rx.try_iter().collect()).unwrap_or_default();
        for ev in events {
            match ev {
                WatchEvent::Db(ch) => {
                    if let Some(c) = &mut self.covers {
                        for b in &ch.updated {
                            c.forget_failure(&b.id, b.front_cover_image());
                        }
                    }
                    self.library.apply_changes(ch);
                    self.library.apply_info_written(&mut self.settings.info_written);
                    if let Some(id) = &self.library.selected_book {
                        if self.library.database().is_some_and(|db| db.get(id).is_none()) {
                            self.library.selected_book = None;
                        }
                    }
                }
                WatchEvent::Replace(db) => {
                    self.library.replace(db);
                    self.library.apply_info_written(&mut self.settings.info_written);
                }
                WatchEvent::DbError(e) => self.library.watch_error = e,
                WatchEvent::Lists { path, result } => {
                    self.library.lists.set(path, result);
                    if self.library.selected_list.is_some() && self.library.selected_list().is_none() {
                        self.select_list(None);
                    }
                }
            }
        }
    }

    pub(crate) fn poll_loader(&mut self, ctx: &Context) {
        if self.library.poll() {
            self.library.apply_info_written(&mut self.settings.info_written);
            self.restart_watch(ctx);
            if std::mem::take(&mut self.scan_after_load) {
                self.start_scan(ctx, false);
            }
        }
        self.poll_scan();
        self.poll_files();
        self.poll_watch();
        // Base ComicRack disponible : une liste résolue sans elle l'est à nouveau, par identifiant.
        if self.library.database().is_some()
            && self.list_loading.is_none()
            && self.list.as_ref().is_some_and(|l| !l.resolved_with_db)
        {
            self.reload_list(ctx);
        }
        self.update.poll();
        if let Some(rx) = &self.list_loading {
            if let Ok(res) = rx.try_recv() {
                self.list_loading = None;
                match res {
                    Ok(mut list) => {
                        let current_path = self.book.as_ref().map(|b| b.path.clone());
                        list.current = current_path.as_deref().and_then(|p| list.position_of(p));
                        let first = list.entries.iter().position(|e| e.file.is_some());
                        let open_first = list.current.is_none();
                        let file = first.and_then(|i| list.entries[i].file.clone());
                        self.list = Some(list);
                        if open_first {
                            if let Some(f) = file {
                                self.open_book(ctx, f);
                            }
                        }
                    }
                    Err(e) => self.error = Some(format!("Liste de lecture illisible : {e}")),
                }
            }
        }

        let Some(book) = &mut self.book else { return };
        let mut failed = None;
        for ev in book.loader.poll() {
            match ev {
                Event::Opened { format, pages, info } => {
                    book.format = Some(format);
                    book.pages = pages;
                    book.info = info.map(|b| *b);
                    book.opened = true;
                    if book.current >= book.pages.len() {
                        book.current = 0;
                    }
                    if let Some(caption) = book.info.as_ref().map(ComicInfo::caption).filter(|c| !c.is_empty()) {
                        book.title = caption;
                    }
                }
                Event::Page { index, result } => {
                    book.requested.remove(&index);
                    match result {
                        Ok((img, size)) => {
                            book.sizes.insert(index, size);
                            let tex = ctx.load_texture(format!("page-{index}"), img, texture_options());
                            book.textures.insert(index, tex);
                            book.errors.remove(&index);
                        }
                        Err(e) => {
                            book.errors.insert(index, e);
                        }
                    }
                }
                Event::Thumb { index, image } => {
                    let tex = image.map(|img| ctx.load_texture(format!("thumb-{index}"), img, TextureOptions::LINEAR));
                    book.thumbs.insert(index, tex);
                }
                Event::Failed(e) => failed = Some(e),
            }
        }
        if let Some(e) = failed {
            let name = book.path.display().to_string();
            self.settings.recent.retain(|p| p != &book.path);
            self.book = None;
            self.error = Some(format!("Impossible d'ouvrir {name}\n\n{e}"));
        }
    }
}
