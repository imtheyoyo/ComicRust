//! Événements venus des fils d'arrière-plan : liste de lecture chargée, pages et vignettes décodées.

use eframe::egui::{Context, TextureOptions};

use crate::app::ComicRustApp;
use crate::book::loader::Event;
use crate::book::texture_options;
use comicrust_core::ComicInfo;

impl ComicRustApp {
    pub(crate) fn poll_loader(&mut self, ctx: &Context) {
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
