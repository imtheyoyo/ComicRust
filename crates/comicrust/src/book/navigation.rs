//! Pagination : pages affichées ensemble (double page, manga), page suivante/précédente,
//! enchaînement des livres d'une liste, préchargement.

use eframe::egui::Context;

use crate::app::ComicRustApp;
use crate::book::loader::Request;
use crate::book::{Book, KEEP_AHEAD, KEEP_BEHIND, PRELOAD_AHEAD};

impl ComicRustApp {
    pub(crate) fn is_wide(book: &Book, i: usize) -> bool {
        book.sizes.get(&i).is_some_and(|[w, h]| *w > *h)
    }

    /// Pages affichées ensemble à partir de `start` (ordre de lecture).
    pub(crate) fn spread(&self, book: &Book, start: usize) -> Vec<usize> {
        let n = book.page_count();
        if n == 0 {
            return vec![];
        }
        let start = start.min(n - 1);
        if !self.settings.double_page
            || start + 1 >= n
            || (self.settings.cover_alone && start == 0)
            || Self::is_wide(book, start)
            || Self::is_wide(book, start + 1)
        {
            return vec![start];
        }
        vec![start, start + 1]
    }

    pub(crate) fn go_to(&mut self, page: usize) {
        if let Some(b) = &mut self.book {
            if b.page_count() > 0 {
                b.current = page.min(b.page_count() - 1);
                self.scroll_to_thumb = true;
            }
        }
    }

    pub(crate) fn next_page(&mut self, ctx: &Context) {
        let Some(book) = &self.book else { return };
        let step = self.spread(book, book.current).len().max(1);
        let target = book.current + step;
        if target < book.page_count() {
            self.go_to(target);
        } else {
            self.next_book(ctx, false);
        }
    }

    pub(crate) fn prev_page(&mut self, ctx: &Context) {
        let Some(book) = &self.book else { return };
        if book.current == 0 {
            self.next_book(ctx, true);
            return;
        }
        let mut target = book.current - 1;
        if self.settings.double_page && target > 0 && self.spread(book, target - 1).len() == 2 {
            target -= 1;
        }
        self.go_to(target);
    }

    /// Livre suivant/précédent de la liste de lecture, s'il y en a une.
    pub(crate) fn next_book(&mut self, ctx: &Context, backward: bool) {
        let Some(list) = &self.list else { return };
        let Some(cur) = list.current else { return };
        if let Some(i) = list.neighbour(cur, backward) {
            if let Some(f) = list.entries[i].file.clone() {
                self.open_book(ctx, f);
            }
        }
    }

    /// Demande au chargeur les pages visibles et proches, libère les autres.
    pub(crate) fn schedule_loading(&mut self) {
        let Some(book) = &self.book else { return };
        if !book.opened {
            return;
        }
        let visible = self.spread(book, book.current);
        let last_visible = *visible.last().unwrap_or(&book.current);
        let mut wanted: Vec<usize> =
            (last_visible + 1..(last_visible + 1 + PRELOAD_AHEAD).min(book.page_count())).rev().collect();
        if book.current > 0 {
            wanted.insert(0, book.current - 1);
        }
        wanted.extend(visible.iter().rev()); // en dernier = traité en premier
        let lo = book.current.saturating_sub(KEEP_BEHIND);
        let hi = book.current + KEEP_AHEAD;

        let book = self.book.as_mut().unwrap();
        book.textures.retain(|&i, _| i >= lo && i <= hi);
        for i in wanted {
            if !book.textures.contains_key(&i) && !book.errors.contains_key(&i) && book.requested.insert(i) {
                book.loader.request(Request::Page(i));
            }
        }
    }
}
