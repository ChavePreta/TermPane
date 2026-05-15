use crate::model::{
    LayoutSnapshot, PaneId, PaneSnapshot, SplitDir, TerminalId, TerminalSnapshot,
};
use crate::pty::SharedPty;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;

/// Árvore de layout interna — folhas referenciam PaneIds, dados ficam num HashMap separado.
#[derive(Debug, Clone)]
pub enum LayoutNode {
    Leaf(PaneId),
    Split {
        dir: SplitDir,
        children: Vec<LayoutNode>,
        ratios: Vec<f32>,
    },
}

impl LayoutNode {
    pub fn snapshot(&self) -> LayoutSnapshot {
        match self {
            LayoutNode::Leaf(id) => LayoutSnapshot::Leaf { id: id.to_string() },
            LayoutNode::Split { dir, children, ratios } => LayoutSnapshot::Split {
                dir: *dir,
                children: children.iter().map(|c| c.snapshot()).collect(),
                ratios: ratios.clone(),
            },
        }
    }

    pub fn leaves(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, acc: &mut Vec<PaneId>) {
        match self {
            LayoutNode::Leaf(id) => acc.push(*id),
            LayoutNode::Split { children, .. } => {
                for c in children {
                    c.collect_leaves(acc);
                }
            }
        }
    }

    /// Replaces the target Leaf with a Split containing [target, new_pane]. Returns `true` if found.
    pub fn split_at(&mut self, target: PaneId, new_pane: PaneId, dir: SplitDir) -> bool {
        match self {
            LayoutNode::Leaf(id) if *id == target => {
                let new_split = LayoutNode::Split {
                    dir,
                    children: vec![LayoutNode::Leaf(target), LayoutNode::Leaf(new_pane)],
                    ratios: vec![0.5, 0.5],
                };
                *self = new_split;
                true
            }
            LayoutNode::Leaf(_) => false,
            LayoutNode::Split { children, .. } => {
                for c in children.iter_mut() {
                    if c.split_at(target, new_pane, dir) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Removes the target Leaf. Returns (found, whole_node_should_be_removed).
    /// When a Split is left with a single child, it collapses to that child.
    pub fn remove_leaf(&mut self, target: PaneId) -> RemoveOutcome {
        match self {
            LayoutNode::Leaf(id) if *id == target => RemoveOutcome::RemoveThisNode,
            LayoutNode::Leaf(_) => RemoveOutcome::NotFound,
            LayoutNode::Split { dir: _, children, ratios } => {
                let mut found_idx: Option<usize> = None;
                for (i, c) in children.iter_mut().enumerate() {
                    match c.remove_leaf(target) {
                        RemoveOutcome::NotFound => continue,
                        RemoveOutcome::RemoveThisNode => {
                            found_idx = Some(i);
                            break;
                        }
                        RemoveOutcome::Modified => return RemoveOutcome::Modified,
                    }
                }
                let Some(i) = found_idx else { return RemoveOutcome::NotFound };
                children.remove(i);
                if ratios.len() > i {
                    ratios.remove(i);
                }
                if children.is_empty() {
                    return RemoveOutcome::RemoveThisNode;
                }
                if children.len() == 1 {
                    // Collapse to the only remaining child.
                    let only = children.pop().unwrap();
                    *self = only;
                    return RemoveOutcome::Modified;
                }
                // Renormalize ratios.
                let sum: f32 = ratios.iter().sum();
                if sum > 0.0 {
                    for r in ratios.iter_mut() {
                        *r /= sum;
                    }
                }
                RemoveOutcome::Modified
            }
        }
    }

    /// Sibling of the target Leaf (for focus after close). DFS — first leaf that is not the target.
    pub fn first_other_leaf(&self, exclude: PaneId) -> Option<PaneId> {
        self.leaves().into_iter().find(|p| *p != exclude)
    }

    pub fn set_ratios(&mut self, parent_node_path: &[usize], new_ratios: Vec<f32>) -> bool {
        let mut node = self;
        for &i in parent_node_path {
            match node {
                LayoutNode::Split { children, .. } => {
                    if i >= children.len() {
                        return false;
                    }
                    node = &mut children[i];
                }
                _ => return false,
            }
        }
        match node {
            LayoutNode::Split { ratios, children, .. } if ratios.len() == new_ratios.len()
                && children.len() == new_ratios.len() =>
            {
                *ratios = new_ratios;
                true
            }
            _ => false,
        }
    }
}

pub enum RemoveOutcome {
    NotFound,
    /// The node should be removed by the caller.
    RemoveThisNode,
    /// The node was mutated in place; nothing else to do.
    Modified,
}

pub struct PaneData {
    pub pty: SharedPty,
    pub foreground_command: Option<String>,
    pub exit_code: Option<i32>,
}

pub struct Terminal {
    pub id: TerminalId,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub panes: HashMap<PaneId, PaneData>,
    pub layout: LayoutNode,
    pub active_pane: PaneId,
}

impl Terminal {
    pub fn snapshot(&self) -> TerminalSnapshot {
        TerminalSnapshot {
            id: self.id.to_string(),
            label: self.label.clone(),
            created_at: self.created_at,
            layout: self.layout.snapshot(),
            panes: self
                .panes
                .iter()
                .map(|(id, data)| PaneSnapshot {
                    id: id.to_string(),
                    foreground_command: data.foreground_command.clone(),
                    exit_code: data.exit_code,
                })
                .collect(),
            active_pane: self.active_pane.to_string(),
        }
    }
}

#[derive(Default)]
pub struct Inner {
    pub terminals: Vec<Terminal>,
    pub active: Option<TerminalId>,
    pub next_label_index: u32,
}

pub struct AppState {
    inner: RwLock<Inner>,
}

impl AppState {
    pub fn new() -> Self {
        let restored = crate::persist::load();
        Self {
            inner: RwLock::new(Inner {
                terminals: Vec::new(),
                active: None,
                next_label_index: restored.next_label_index,
            }),
        }
    }

    pub fn allocate_label(&self) -> String {
        let mut g = self.inner.write();
        g.next_label_index += 1;
        let idx = g.next_label_index;
        drop(g);
        let snapshot = crate::persist::PersistedState {
            schema_version: 1,
            next_label_index: idx,
        };
        std::thread::spawn(move || {
            if let Err(e) = crate::persist::save(&snapshot) {
                tracing::debug!("persist save failed: {e}");
            }
        });
        format!("Term#{}", idx)
    }

    pub fn insert(&self, terminal: Terminal) {
        let mut g = self.inner.write();
        let id = terminal.id;
        g.terminals.push(terminal);
        if g.active.is_none() {
            g.active = Some(id);
        }
    }

    /// Renames a terminal. Returns `true` if found.
    pub fn rename_terminal(&self, id: TerminalId, label: String) -> bool {
        let mut g = self.inner.write();
        if let Some(t) = g.terminals.iter_mut().find(|t| t.id == id) {
            t.label = label;
            return true;
        }
        false
    }

    /// Reorders terminals to match the provided order. Returns `true`
    /// if `ids` is exactly a permutation of the current set.
    pub fn reorder(&self, ids: &[TerminalId]) -> bool {
        let mut g = self.inner.write();
        if ids.len() != g.terminals.len() {
            return false;
        }
        // Verifica que cada id existe exatamente uma vez.
        let mut new_order: Vec<Terminal> = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(pos) = g.terminals.iter().position(|t| t.id == *id) {
                new_order.push(g.terminals.remove(pos));
            } else {
                // Some id is not part of the current set — restore and abort.
                g.terminals.extend(new_order);
                return false;
            }
        }
        g.terminals = new_order;
        true
    }

    /// Removes a whole terminal (killing all its panes). Returns the removed terminal.
    pub fn remove_terminal(&self, id: TerminalId) -> Option<Terminal> {
        let mut g = self.inner.write();
        let pos = g.terminals.iter().position(|t| t.id == id)?;
        let term = g.terminals.remove(pos);
        if g.active == Some(id) {
            g.active = g.terminals.first().map(|t| t.id);
        }
        Some(term)
    }

    pub fn snapshots(&self) -> Vec<TerminalSnapshot> {
        self.inner.read().terminals.iter().map(|t| t.snapshot()).collect()
    }

    pub fn terminal_snapshot(&self, id: TerminalId) -> Option<TerminalSnapshot> {
        self.inner
            .read()
            .terminals
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.snapshot())
    }

    pub fn pty_of(&self, pane_id: PaneId) -> Option<SharedPty> {
        let g = self.inner.read();
        for t in &g.terminals {
            if let Some(p) = t.panes.get(&pane_id) {
                return Some(p.pty.clone());
            }
        }
        None
    }

    /// Applies a split: creates a new pane (pty) on the right terminal and updates the layout.
    pub fn split(
        &self,
        pane_id: PaneId,
        new_pane_id: PaneId,
        new_pty: SharedPty,
        dir: SplitDir,
    ) -> Option<TerminalId> {
        let mut g = self.inner.write();
        for t in g.terminals.iter_mut() {
            if t.panes.contains_key(&pane_id) {
                if t.layout.split_at(pane_id, new_pane_id, dir) {
                    t.panes.insert(
                        new_pane_id,
                        PaneData {
                            pty: new_pty,
                            foreground_command: None,
                            exit_code: None,
                        },
                    );
                    t.active_pane = new_pane_id;
                    return Some(t.id);
                }
                return None;
            }
        }
        None
    }

    /// Removes a pane. If it was the last pane in the terminal, returns
    /// `CloseResult::TerminalEmpty` so the caller can delete the terminal.
    pub fn close_pane(&self, pane_id: PaneId) -> CloseResult {
        let mut g = self.inner.write();
        for t in g.terminals.iter_mut() {
            if !t.panes.contains_key(&pane_id) {
                continue;
            }
            let next_active = t.layout.first_other_leaf(pane_id);
            let outcome = t.layout.remove_leaf(pane_id);
            let pane_data = t.panes.remove(&pane_id);

            match outcome {
                RemoveOutcome::RemoveThisNode | RemoveOutcome::NotFound
                    if t.panes.is_empty() =>
                {
                    return CloseResult::TerminalEmpty {
                        terminal_id: t.id,
                        removed_pane: pane_data,
                    };
                }
                _ => {
                    if t.active_pane == pane_id {
                        if let Some(np) = next_active {
                            t.active_pane = np;
                        }
                    }
                    return CloseResult::PaneRemoved {
                        terminal_id: t.id,
                        removed_pane: pane_data,
                    };
                }
            }
        }
        CloseResult::NotFound
    }

    pub fn focus_pane(&self, pane_id: PaneId) -> Option<TerminalId> {
        let mut g = self.inner.write();
        for t in g.terminals.iter_mut() {
            if t.panes.contains_key(&pane_id) {
                t.active_pane = pane_id;
                return Some(t.id);
            }
        }
        None
    }

    pub fn set_layout_ratios(
        &self,
        terminal_id: TerminalId,
        path: Vec<usize>,
        ratios: Vec<f32>,
    ) -> bool {
        let mut g = self.inner.write();
        if let Some(t) = g.terminals.iter_mut().find(|t| t.id == terminal_id) {
            return t.layout.set_ratios(&path, ratios);
        }
        false
    }

    pub fn all_pane_ids(&self) -> Vec<(TerminalId, PaneId, Option<u32>, Option<i32>)> {
        let g = self.inner.read();
        let mut out = Vec::new();
        for t in &g.terminals {
            for (pid, p) in &t.panes {
                out.push((t.id, *pid, p.pty.shell_pid, p.pty.foreground_pgid()));
            }
        }
        out
    }

    pub fn active(&self) -> Option<TerminalId> {
        self.inner.read().active
    }

    pub fn set_active(&self, id: TerminalId) -> bool {
        let mut g = self.inner.write();
        if g.terminals.iter().any(|t| t.id == id) {
            g.active = Some(id);
            true
        } else {
            false
        }
    }

    /// Updates a pane's foreground_command. Returns `true` if it changed.
    pub fn set_foreground(&self, pane_id: PaneId, cmd: Option<String>) -> bool {
        let mut g = self.inner.write();
        for t in g.terminals.iter_mut() {
            if let Some(p) = t.panes.get_mut(&pane_id) {
                if p.foreground_command != cmd {
                    p.foreground_command = cmd;
                    return true;
                }
                return false;
            }
        }
        false
    }
}

pub enum CloseResult {
    NotFound,
    /// Pane removed; terminal is still alive.
    PaneRemoved {
        terminal_id: TerminalId,
        removed_pane: Option<PaneData>,
    },
    /// Removed pane was the last one; the caller should close the terminal.
    TerminalEmpty {
        terminal_id: TerminalId,
        removed_pane: Option<PaneData>,
    },
}
