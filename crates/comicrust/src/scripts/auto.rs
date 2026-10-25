//! File d'attente des automatismes (`Startup`, `BookOpened`, `BookClosed`, `NewBooks`) :
//! **un seul automatisme à la fois par hook**, les suivants attendent leur tour (dans
//! l'ordre). Des hooks différents peuvent tourner en même temps.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use comicrust_core::scripting::{Hook, ScriptEntry};

/// Automatisme à lancer.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoJob {
    pub entry: ScriptEntry,
    pub hook: Hook,
    pub book_ids: Vec<String>,
}

#[derive(Debug, Default)]
pub struct AutoQueue {
    waiting: BTreeMap<Hook, VecDeque<AutoJob>>,
    running: BTreeSet<Hook>,
}

impl AutoQueue {
    /// Au-delà, les nouveaux déclenchements d'un hook sont ignorés (livres ouverts en
    /// rafale pendant qu'un script lent tourne…).
    pub const MAX_WAITING: usize = 50;

    /// Ajoute un automatisme ; `false` s'il est ignoré (file pleine).
    pub fn push(&mut self, job: AutoJob) -> bool {
        let q = self.waiting.entry(job.hook).or_default();
        if q.len() >= Self::MAX_WAITING {
            return false;
        }
        q.push_back(job);
        true
    }

    /// Automatismes à lancer maintenant : le premier de chaque hook libre, marqué « en
    /// cours ». `allowed` faux (transfert en cours…) : rien n'est lancé, tout attend.
    pub fn start_ready(&mut self, allowed: bool) -> Vec<AutoJob> {
        if !allowed {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (hook, q) in &mut self.waiting {
            if self.running.contains(hook) {
                continue;
            }
            if let Some(job) = q.pop_front() {
                self.running.insert(*hook);
                out.push(job);
            }
        }
        self.waiting.retain(|_, q| !q.is_empty());
        out
    }

    /// L'automatisme en cours de `hook` est terminé : le suivant pourra partir.
    pub fn finished(&mut self, hook: Hook) {
        self.running.remove(&hook);
    }

    pub fn waiting(&self) -> usize {
        self.waiting.values().map(VecDeque::len).sum()
    }

    pub fn running(&self) -> usize {
        self.running.len()
    }

    /// Oublie les automatismes en attente (bibliothèque changée, automatismes coupés).
    pub fn clear_waiting(&mut self) {
        self.waiting.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(name: &str, hook: Hook) -> AutoJob {
        AutoJob {
            entry: ScriptEntry {
                file: format!("/s/{name}.py").into(),
                rel_path: format!("{name}.py"),
                function: name.into(),
                name: name.into(),
                hooks: vec![hook],
                key: None,
                description: None,
                image: None,
                line: 1,
            },
            hook,
            book_ids: vec![],
        }
    }

    #[test]
    fn un_seul_automatisme_a_la_fois_par_hook() {
        let mut q = AutoQueue::default();
        assert!(q.push(job("a", Hook::BookOpened)));
        assert!(q.push(job("b", Hook::BookOpened)));
        assert!(q.push(job("c", Hook::NewBooks)));
        assert!(q.start_ready(false).is_empty(), "transfert en cours : tout attend");
        assert_eq!(q.waiting(), 3);
        let first: Vec<String> = q.start_ready(true).into_iter().map(|j| j.entry.name).collect();
        assert_eq!(first, ["a", "c"], "un par hook, hooks différents en parallèle");
        assert_eq!(q.running(), 2);
        assert!(q.start_ready(true).is_empty(), "b attend la fin de a");
        q.finished(Hook::BookOpened);
        let next: Vec<String> = q.start_ready(true).into_iter().map(|j| j.entry.name).collect();
        assert_eq!(next, ["b"]);
        q.finished(Hook::BookOpened);
        q.finished(Hook::NewBooks);
        assert_eq!((q.waiting(), q.running()), (0, 0));
    }

    #[test]
    fn file_pleine() {
        let mut q = AutoQueue::default();
        for _ in 0..AutoQueue::MAX_WAITING {
            assert!(q.push(job("a", Hook::BookClosed)));
        }
        assert!(!q.push(job("a", Hook::BookClosed)));
        assert!(q.push(job("a", Hook::Startup)), "autre hook, autre file");
        q.clear_waiting();
        assert_eq!(q.waiting(), 0);
    }
}
