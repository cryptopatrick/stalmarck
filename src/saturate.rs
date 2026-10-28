//! 0-saturation (§6), the Dilemma rule (§7) and k-saturation (§8), in the
//! canonical Track A order.

use std::collections::VecDeque;
use std::time::Instant;

use crate::relation::{Relation, Union};
use crate::rules::local_closure;
use crate::triples::{Lit, Triple};

/// Solver counters (SPEC-Z6, SPEC-K4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// `deductions_attempted − deductions_successful`.
    pub fn redundant(&self) -> u64 {
        self.deductions_attempted - self.deductions_successful
    }
}

/// Which resource limit stopped saturation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Limit {
    Branches,
    Time,
}

pub(crate) struct Budget {
    pub max_branches: Option<u64>,
    pub deadline: Option<Instant>,
}

pub(crate) struct Engine<'t> {
    triples: &'t [Triple],
    occurrences: Vec<Vec<u32>>,
    queue: VecDeque<u32>,
    queued: Vec<bool>,
    scan: Vec<u32>,
    affected: Vec<u32>,
    derived: Vec<(Lit, Lit)>,
    budget: Budget,
    pub stats: Stats,
}

impl<'t> Engine<'t> {
    pub fn new(
        node_count: u32,
        triples: &'t [Triple],
        budget: Budget,
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
            occurrences,
            queue: VecDeque::new(),
            queued: vec![false; triples.len()],
            scan: Vec::new(),
            affected: Vec::new(),
            derived: Vec::new(),
            budget,
            stats: Stats::default(),
        }
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
                let merged = self.dilemma(rel, v, k - 1)?;
                if merged.canonical() != rel.canonical() {
                    self.stats.dilemma_productive += 1;
                    changed = true;
                    *rel = merged;
                }
            }
            if !changed || rel.is_contradiction() {
                return Ok(());
            }
        }
    }

    /// `sat_j(R(v ≡ ⊥)) ⊓ sat_j(R(v ≡ ⊤))` (SPEC-M1). The merged relation
    /// is already `sat_j`-closed (SPEC-M3).
    fn dilemma(
        &mut self,
        rel: &Relation,
        v: u32,
        j: u32,
    ) -> Result<Relation, Limit> {
        let mut branches = [rel.clone(), rel.clone()];
        for (r, c) in branches.iter_mut().zip([Lit::FALSE, Lit::TRUE]) {
            self.assert_eq(r, Lit::new(v, false), c);
            self.saturate(r, j)?;
        }
        Ok(Relation::meet(&branches[0], &branches[1]))
    }
}
