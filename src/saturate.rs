//! 0-saturation (§6), the Dilemma rule (§7) and k-saturation (§8), in the
//! canonical Track A order.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::relation::{Checkpoint, Relation, Union};
use crate::rules::local_closure;
use crate::triples::{Lit, Triple};

/// Solver counters (SPEC-Z6, SPEC-K4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Stats {
    /// Worklist pops.
    pub triples_processed: u64,
    /// Equivalences proposed by the local rules.
    pub deductions_attempted: u64,
    /// Proposed equivalences that changed the relation.
    pub deductions_successful: u64,
    /// Class unions, including unions with ⊤ and dilemma assumptions.
    pub merges: u64,
    /// Worklist pushes.
    pub worklist_insertions: u64,
    /// Worklist pops.
    pub worklist_removals: u64,
    /// Dilemma rule applications (each explores two branches).
    pub dilemma_branches: u64,
    /// Dilemma applications whose merge added at least one equivalence.
    pub dilemma_productive: u64,
}

impl Stats {
    pub(crate) fn add(&mut self, o: &Stats) {
        self.triples_processed += o.triples_processed;
        self.deductions_attempted += o.deductions_attempted;
        self.deductions_successful += o.deductions_successful;
        self.merges += o.merges;
        self.worklist_insertions += o.worklist_insertions;
        self.worklist_removals += o.worklist_removals;
        self.dilemma_branches += o.dilemma_branches;
        self.dilemma_productive += o.dilemma_productive;
    }

    /// `deductions_attempted − deductions_successful`.
    #[must_use]
    pub fn redundant(&self) -> u64 {
        self.deductions_attempted - self.deductions_successful
    }
}

/// What stopped saturation early.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Limit {
    Branches,
    Time,
    /// The external branch executor failed.
    Backend(String),
}

pub(crate) struct Budget {
    pub max_branches: Option<u64>,
    pub deadline: Option<Instant>,
}

/// The result of one branch run outside this engine.
pub(crate) struct BranchOutcome {
    /// The saturated branch, or `None` if it is contradictory.
    pub relation: Option<Relation>,
    /// Work done inside the branch, nested dilemmas included.
    pub stats: Stats,
    /// The branch stopped at the branch limit.
    pub limited: bool,
}

/// Runs whole Dilemma branches elsewhere (the Scryer back end, ADR-0005).
/// The engine then drives only the top-level dilemmas and their merges.
pub(crate) trait ExternalBranches {
    /// Saturates `parent(v ≡ value)` to `depth`, allowing at most
    /// `max_branches` nested dilemmas.
    fn branch(
        &mut self,
        parent: &Relation,
        v: u32,
        value: bool,
        depth: u32,
        max_branches: Option<u64>,
    ) -> Result<BranchOutcome, String>;
}

/// What the rollback dilemma remembers about its first branch.
struct FirstBranch {
    /// `(node, root, parity)` for every node whose class changed, by node.
    snapshot: Vec<(u32, u32, u8)>,
    /// The branch's links, to rebuild it if the second branch fails.
    links: Vec<(u32, u32, u8)>,
}

/// How a dilemma keeps its parent state while it explores the branches
/// (PRD §10, ADR-0004).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum BranchState {
    /// One mutable state with an undo trail (the Track A baseline).
    #[default]
    Rollback,
    /// A copy of the state per branch.
    Clone,
}

pub(crate) struct Engine<'t> {
    triples: &'t [Triple],
    branch_state: BranchState,
    external: Option<&'t mut dyn ExternalBranches>,
    occurrences: Vec<Vec<u32>>,
    queue: VecDeque<u32>,
    queued: Vec<bool>,
    scan: Vec<u32>,
    affected: Vec<u32>,
    derived: Vec<(Lit, Lit)>,
    budget: Budget,
    pub stats: Stats,
    pub merge_time: Duration,
}

impl<'t> Engine<'t> {
    pub fn new(
        node_count: u32,
        triples: &'t [Triple],
        budget: Budget,
        branch_state: BranchState,
    ) -> Self {
        let mut occurrences = vec![Vec::new(); node_count as usize];
        for (i, t) in triples.iter().enumerate() {
            let mut nodes = [t.out, t.a.node(), t.b.node()];
            nodes.sort_unstable();
            for (j, &n) in nodes.iter().enumerate() {
                if n != 0 && (j == 0 || nodes[j - 1] != n) {
                    occurrences[n as usize].push(i as u32);
                }
            }
        }
        Engine {
            triples,
            branch_state,
            external: None,
            occurrences,
            queue: VecDeque::new(),
            queued: vec![false; triples.len()],
            scan: Vec::new(),
            affected: Vec::new(),
            derived: Vec::new(),
            budget,
            stats: Stats::default(),
            merge_time: Duration::ZERO,
        }
    }

    /// Sends every dilemma branch to `external`.
    pub fn set_external(&mut self, external: &'t mut dyn ExternalBranches) {
        self.external = Some(external);
    }

    fn push(&mut self, t: u32) {
        if !self.queued[t as usize] {
            self.queued[t as usize] = true;
            self.queue.push_back(t);
            self.stats.worklist_insertions += 1;
        }
    }

    fn clear_queue(&mut self) {
        for t in self.queue.drain(..) {
            self.queued[t as usize] = false;
        }
    }

    /// Enqueues every triple mentioning a node of the last scan class, in
    /// ascending id order (SPEC-Z3, SPEC-Z4).
    fn enqueue_affected(&mut self) {
        self.affected.clear();
        for &v in &self.scan {
            self.affected.extend(&self.occurrences[v as usize]);
        }
        self.affected.sort_unstable();
        self.affected.dedup();
        for i in 0..self.affected.len() {
            self.push(self.affected[i]);
        }
    }

    /// Asserts `a ≡ b` and enqueues the affected triples.
    fn assert_eq(&mut self, rel: &mut Relation, a: Lit, b: Lit) -> Union {
        let u = rel.union(a, b, &mut self.scan);
        if u == Union::Merged {
            self.stats.merges += 1;
            self.enqueue_affected();
        }
        u
    }

    /// The initial relation of SPEC-D7 with a full worklist.
    pub fn start(&mut self, rel: &mut Relation, root: Lit, value: bool) {
        let c = if value { Lit::TRUE } else { Lit::FALSE };
        self.assert_eq(rel, root, c);
        for t in 0..self.triples.len() as u32 {
            self.push(t);
        }
    }

    fn check_time(&self) -> Result<(), Limit> {
        match self.budget.deadline {
            Some(d) if Instant::now() >= d => Err(Limit::Time),
            _ => Ok(()),
        }
    }

    /// 0-saturation of `rel` from the current worklist (SPEC-Z2).
    fn saturate0(&mut self, rel: &mut Relation) -> Result<(), Limit> {
        while let Some(t) = self.queue.pop_front() {
            self.queued[t as usize] = false;
            self.stats.worklist_removals += 1;
            self.stats.triples_processed += 1;
            if self.stats.triples_processed.is_multiple_of(1024)
                && let Err(e) = self.check_time()
            {
                self.clear_queue();
                return Err(e);
            }
            let triple = self.triples[t as usize];
            let mut derived = std::mem::take(&mut self.derived);
            let ok = local_closure(rel, &triple, &mut derived);
            if !ok {
                rel.set_contradiction();
            }
            for &(a, b) in &derived {
                if rel.is_contradiction() {
                    break;
                }
                self.stats.deductions_attempted += 1;
                if self.assert_eq(rel, a, b) != Union::Redundant {
                    self.stats.deductions_successful += 1;
                }
            }
            self.derived = derived;
            if rel.is_contradiction() {
                self.clear_queue();
                return Ok(());
            }
        }
        Ok(())
    }

    /// k-saturation (SPEC-K3). The worklist must hold the triples affected
    /// since `rel` was last 0-saturated.
    pub fn saturate(
        &mut self,
        rel: &mut Relation,
        k: u32,
    ) -> Result<(), Limit> {
        if k == 0 {
            return self.saturate0(rel);
        }
        self.saturate(rel, k - 1)?;
        loop {
            let mut changed = false;
            for v in 1..rel.node_count() {
                if rel.is_contradiction() {
                    return Ok(());
                }
                if rel.value(v).is_some() || rel.class_min(v) != v {
                    continue;
                }
                self.check_time()?;
                if let Some(max) = self.budget.max_branches
                    && self.stats.dilemma_branches >= max
                {
                    return Err(Limit::Branches);
                }
                self.stats.dilemma_branches += 1;
                let productive = if self.external.is_some() {
                    self.dilemma_external(rel, v, k - 1)?
                } else {
                    match self.branch_state {
                        BranchState::Rollback => {
                            self.dilemma(rel, v, k - 1)?
                        }
                        BranchState::Clone => {
                            self.dilemma_clone(rel, v, k - 1)?
                        }
                    }
                };
                if productive {
                    self.stats.dilemma_productive += 1;
                    changed = true;
                }
            }
            if !changed || rel.is_contradiction() {
                return Ok(());
            }
        }
    }

    /// Saturates `rel(v ≡ c)` to depth `j`. On a resource limit the
    /// branch is rolled back first, so `rel` stays sound.
    fn branch(
        &mut self,
        rel: &mut Relation,
        cp: Checkpoint,
        v: u32,
        c: Lit,
        j: u32,
    ) -> Result<(), Limit> {
        self.assert_eq(rel, Lit::new(v, false), c);
        self.saturate(rel, j).inspect_err(|_| rel.rollback(cp))
    }

    /// `sat_j(R(v ≡ ⊥)) ⊓ sat_j(R(v ≡ ⊤))` (SPEC-M1) on one mutable state
    /// with rollback (ADR-0004). The meet is computed only over nodes whose
    /// class changed in both branches (ADR-0007, option 3). Installing it
    /// needs no propagation (SPEC-M3). Returns whether `rel` grew.
    fn dilemma(
        &mut self,
        rel: &mut Relation,
        v: u32,
        j: u32,
    ) -> Result<bool, Limit> {
        let cp = rel.checkpoint();
        self.branch(rel, cp, v, Lit::FALSE, j)?;
        let first = if rel.is_contradiction() {
            None
        } else {
            // (node, root, parity) for every changed node, by node.
            let snapshot: Vec<(u32, u32, u8)> = rel
                .nodes_changed_since(cp)
                .into_iter()
                .map(|u| {
                    let (r, p) = rel.find(u);
                    (u, r, p)
                })
                .collect();
            Some(FirstBranch { snapshot, links: rel.links_since(cp) })
        };
        rel.rollback(cp);
        self.branch(rel, cp, v, Lit::TRUE, j)?;
        let start = Instant::now();
        let grew = self.merge(rel, cp, first);
        self.merge_time += start.elapsed();
        Ok(grew)
    }

    /// Installs `R₁ ⊓ R₂` where `rel` holds R₂ and `first` describes R₁.
    fn merge(
        &mut self,
        rel: &mut Relation,
        cp: Checkpoint,
        first: Option<FirstBranch>,
    ) -> bool {
        let Some(FirstBranch { snapshot, links }) = first else {
            // ⊥⊥ ⊓ R₂ = R₂: keep the second branch as it is.
            return rel.changed_since(cp);
        };
        if rel.is_contradiction() {
            // R₁ ⊓ ⊥⊥ = R₁: rebuild the first branch from its links.
            rel.rollback(cp);
            for (c, r, p) in links {
                rel.union(
                    Lit::new(c, false),
                    Lit::new(r, p == 1),
                    &mut self.scan,
                );
            }
            return rel.changed_since(cp);
        }
        // Nodes changed in both branches (a merge-join of two sorted
        // lists), keyed by SPEC-R6 and grouped by sorting.
        let mut keyed: Vec<(u32, u32, u8, u32, u8)> = Vec::new();
        let mut it = snapshot.iter().peekable();
        for u in rel.nodes_changed_since(cp) {
            while it.next_if(|&&(w, _, _)| w < u).is_some() {}
            if let Some(&&(w, r1, p1)) = it.peek()
                && w == u
            {
                let (r2, p2) = rel.find(u);
                keyed.push((r1, r2, p1 ^ p2, u, p1));
            }
        }
        keyed.sort_unstable();
        let mut unions = Vec::new();
        for group in keyed.chunk_by(|a, b| (a.0, a.1, a.2) == (b.0, b.1, b.2))
        {
            let (_, _, _, w, pw) = group[0];
            for &(_, _, _, u, p1) in &group[1..] {
                unions.push((w, u, pw ^ p1));
            }
        }
        rel.rollback(cp);
        let mut grew = false;
        for (w, u, p) in unions {
            let lw = Lit::new(w, false);
            let r = rel.union(lw, Lit::new(u, p == 1), &mut self.scan);
            debug_assert_ne!(r, Union::Contradiction);
            grew |= r == Union::Merged;
        }
        grew
    }

    /// The same as [`Engine::dilemma`], with both branches run by the
    /// external executor and merged here.
    fn dilemma_external(
        &mut self,
        rel: &mut Relation,
        v: u32,
        j: u32,
    ) -> Result<bool, Limit> {
        let mut branches = Vec::with_capacity(2);
        for value in [false, true] {
            let max = self
                .budget
                .max_branches
                .map(|m| m.saturating_sub(self.stats.dilemma_branches));
            let ext = self.external.as_deref_mut().expect("external executor");
            let out =
                ext.branch(rel, v, value, j, max).map_err(Limit::Backend)?;
            self.stats.add(&out.stats);
            if out.limited {
                return Err(Limit::Branches);
            }
            branches.push(out.relation);
        }
        let start = Instant::now();
        let r2 = branches.pop().unwrap();
        let r1 = branches.pop().unwrap();
        let merged = match (r1, r2) {
            (None, None) => {
                let mut r = rel.clone();
                r.set_contradiction();
                r
            }
            (None, Some(r)) | (Some(r), None) => r,
            (Some(a), Some(b)) => Relation::meet(&a, &b),
        };
        let grew = merged.canonical() != rel.canonical();
        *rel = merged;
        self.merge_time += start.elapsed();
        Ok(grew)
    }

    /// The same as [`Engine::dilemma`], with each branch on its own copy of
    /// the state (Strategy B of PRD §10).
    fn dilemma_clone(
        &mut self,
        rel: &mut Relation,
        v: u32,
        j: u32,
    ) -> Result<bool, Limit> {
        let mut branches = [rel.clone(), rel.clone()];
        for (r, c) in branches.iter_mut().zip([Lit::FALSE, Lit::TRUE]) {
            self.assert_eq(r, Lit::new(v, false), c);
            self.saturate(r, j)?;
        }
        let start = Instant::now();
        let merged = Relation::meet(&branches[0], &branches[1]);
        let grew = merged.canonical() != rel.canonical();
        *rel = merged;
        self.merge_time += start.elapsed();
        Ok(grew)
    }
}
