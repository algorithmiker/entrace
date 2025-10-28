use itertools::Itertools;
use roaring::{MultiOps, RoaringBitmap as Roaring};
use std::collections::HashMap;
use std::fmt::{Debug, Write};
use std::{
    cmp::Ordering,
    collections::{HashSet, VecDeque},
};

pub type FiltersetId = usize;
pub type PredicateId = usize;
#[derive(Debug)]
pub struct Predicate<T> {
    pub attr: String,
    pub rel: Ordering,
    pub constant: T,
}
impl<T> Predicate<T> {
    pub fn new(attrname: impl ToString, rel: Ordering, constant: T) -> Self {
        Self { attr: attrname.to_string(), rel, constant }
    }
}
#[derive(Debug)]
pub enum Filterset {
    Dead,
    Primitive(Roaring),
    BlackBox(FiltersetId),
    RelDnf(Vec<Vec<PredicateId>>, FiltersetId),
    // TODO: HashSet instead of vec? could be faster
    And(Vec<FiltersetId>),
    Or(Vec<FiltersetId>),
    Not(FiltersetId),
}
impl Filterset {
    pub fn children(&self) -> ChildrenRef<'_> {
        match self {
            Filterset::Dead | Filterset::Primitive(_) => ChildrenRef::None,
            Filterset::Not(a) | Filterset::BlackBox(a) | Filterset::RelDnf(_, a) => {
                ChildrenRef::One(*a)
            }
            Filterset::And(i) | Filterset::Or(i) => ChildrenRef::Many(i),
        }
    }
}
#[derive(Debug)]
pub enum RewriteAction {
    None,
    // Pointer of outer and, and indices to inner ands in its item list
    CompressAnd(FiltersetId, Vec<FiltersetId>),
    CompressOr(FiltersetId, Vec<FiltersetId>),
    EliminateNotNot(FiltersetId, FiltersetId, FiltersetId),
    /// Outer DNF, inner DNF, inner DNF source
    DnfDnf(FiltersetId, FiltersetId, FiltersetId),
    MergeDnfsInOr(FiltersetId, HashMap<usize, Vec<usize>>),
    MergeDnfsInAnd(FiltersetId, HashMap<usize, Vec<usize>>),
    /// Or([A]) -> A
    EliminateSingleOr(FiltersetId),
    EliminateSingleAnd(FiltersetId),
}
pub enum ChildrenRef<'a> {
    None,
    One(FiltersetId),
    Many(&'a [FiltersetId]),
}

// I don't know what would be optimal, this is just going by feeling
const MAX_DNF_CLAUSES: usize = 128;
const DNFS_IN_AND_MERGE_MAX_CLAUSES: usize = MAX_DNF_CLAUSES / 2;
pub struct Evaluator<T> {
    pool: Vec<Filterset>,
    pub predicates: Vec<Predicate<T>>,
    pub results: HashMap<FiltersetId, Roaring>,
}
impl<T> Evaluator<T> {
    pub fn new() -> Self {
        Self { pool: vec![], predicates: vec![], results: HashMap::new() }
    }
    pub fn is_and(&self, id: FiltersetId) -> bool {
        matches!(self.pool[id], Filterset::And(_))
    }
    pub fn is_or(&self, id: FiltersetId) -> bool {
        matches!(self.pool[id], Filterset::Or(_))
    }
    pub fn is_dnf(&self, id: FiltersetId) -> bool {
        matches!(self.pool[id], Filterset::RelDnf(..))
    }
    /// Take the value of Filterset::RelDnf at id, and replace it with Dead.
    pub fn dead_and_take_dnf(&mut self, id: FiltersetId) -> (Vec<Vec<usize>>, FiltersetId) {
        let Filterset::RelDnf(clauses, src) =
            std::mem::replace(&mut self.pool[id], Filterset::Dead)
        else {
            unreachable!()
        };
        (clauses, src)
    }
    pub fn new_dnf(&mut self, clauses: Vec<Vec<Predicate<T>>>, src: FiltersetId) -> FiltersetId {
        let mut out_clauses: Vec<Vec<PredicateId>> = vec![];
        for inner in clauses {
            let and_joined_clause = inner.into_iter().map(|x| self.new_predicate(x));
            out_clauses.push(and_joined_clause.collect());
        }
        self.new_filterset(Filterset::RelDnf(out_clauses, src))
    }
    pub fn new_filterset(&mut self, f: Filterset) -> FiltersetId {
        self.pool.push(f);
        self.pool.len() - 1
    }
    pub fn new_predicate(&mut self, t: Predicate<T>) -> PredicateId {
        self.predicates.push(t);
        self.predicates.len() - 1
    }
    pub fn len_of_merged_dnf(&self, dnfs: impl Iterator<Item = FiltersetId>) -> usize {
        dnfs.filter_map(|x| match self.pool[x] {
            Filterset::RelDnf(ref items, _) => Some(items.len()),
            _ => None,
        })
        .product()
    }
    pub fn decide_rewrite_action(&self, id: FiltersetId) -> RewriteAction {
        match &self.pool[id] {
            Filterset::And(items) => {
                if items.len() == 1 {
                    return RewriteAction::EliminateSingleAnd(id);
                }
                let ands: Vec<FiltersetId> =
                    items.iter().copied().filter(|p| self.is_and(*p)).collect();
                if !ands.is_empty() {
                    return RewriteAction::CompressAnd(id, ands);
                }
                // Try to merge And([RelDnf(c, A), RelDnf(c2, A), RelDnf(c3, B), RelDnf(c4, B)])
                //           to And([RelDnf([c & c2], A), RelDnf(c3+c4, B)])
                // will miss duplicate sources, we can't really do anything about that here.
                // that'd involve a source deduplication step before rewriting anything else,
                // but its not clear how to do that
                let dnf_by_source: HashMap<FiltersetId, Vec<FiltersetId>> = items
                    .iter()
                    .filter_map(|x| match &self.pool[*x] {
                        Filterset::RelDnf(_cs, src) => Some((*src, *x)),
                        _ => None,
                    })
                    .into_group_map();
                let can_merge_something = dnf_by_source.iter().any(|(_, ids)| {
                    ids.len() > 1
                        && dnf_by_source.iter().any(|(_, ds)| {
                            self.len_of_merged_dnf(ds.iter().copied())
                                < DNFS_IN_AND_MERGE_MAX_CLAUSES
                        })
                });
                if can_merge_something {
                    return RewriteAction::MergeDnfsInAnd(id, dnf_by_source);
                }
            }
            Filterset::Or(items) => {
                if items.len() == 1 {
                    return RewriteAction::EliminateSingleOr(id);
                }
                let ors: Vec<usize> = items.iter().copied().filter(|x| self.is_or(*x)).collect();
                if !ors.is_empty() {
                    return RewriteAction::CompressOr(id, ors);
                }
                // Try to merge Or([RelDnf(c, A), RelDnf(c2, A), RelDnf(c3, B), RelDnf(c4, B)])
                //           to Or([RelDnf(c+c2, A), RelDnf(c3+c4, B)])
                // will miss duplicate sources, we can't really do anything about that here.
                // that'd involve a source deduplication step before rewriting anything else,
                // but its not clear how to do that
                let dnf_by_source: HashMap<FiltersetId, Vec<FiltersetId>> = items
                    .iter()
                    .filter_map(|x| match &self.pool[*x] {
                        Filterset::RelDnf(_cs, src) => Some((*src, *x)),
                        _ => None,
                    })
                    .into_group_map();
                let can_merge_something = dnf_by_source.iter().any(|(_, ids)| ids.len() > 1);
                if can_merge_something {
                    return RewriteAction::MergeDnfsInOr(id, dnf_by_source);
                }
            }

            Filterset::Not(y) => {
                if let Filterset::Not(q) = &self.pool[*y] {
                    return RewriteAction::EliminateNotNot(id, *y, *q);
                }
            }
            Filterset::RelDnf(c1, src) => {
                if let Filterset::RelDnf(c2, src2) = &self.pool[*src]
                    && c1.len().saturating_mul(c2.len()) < MAX_DNF_CLAUSES
                {
                    return RewriteAction::DnfDnf(id, *src, *src2);
                }
            }
            _ => (),
        }
        RewriteAction::None
    }
    /// Returns the action which ended up being executed
    pub fn rewrite_one(&mut self, id: FiltersetId) -> RewriteAction {
        let action = self.decide_rewrite_action(id);
        self.do_rewrite_action(&action);
        action
    }
    /// Very important invariant: we assume anyone who has the index of a Filterset "owns" it,
    /// so we cannot create dangling references (bad references to Dead values) by rewriting.
    /// This is not true for primitives (there can be multiple references to a Primitive), but we
    /// never rewrite Primitives.
    pub fn do_rewrite_action(&mut self, action: &RewriteAction) {
        match action {
            RewriteAction::None => (),
            RewriteAction::CompressAnd(id, inner_ands) => {
                let Filterset::And(ref items) = self.pool[*id] else { unreachable!() };
                // can probably be done better
                let mut set: HashSet<FiltersetId> = HashSet::from_iter(items.iter().copied());
                for ptr in inner_ands.iter() {
                    set.remove(ptr);
                    let Filterset::And(ref others) = self.pool[items[*ptr]] else { unreachable!() };
                    set.extend(others);
                }
                let Filterset::And(ref mut items) = self.pool[*id] else { unreachable!() };
                items.clear();
                items.extend(set);
                for ptr in inner_ands {
                    self.pool[*ptr] = Filterset::Dead;
                }
            }
            RewriteAction::CompressOr(id, inner_ors) => {
                let Filterset::Or(ref items) = self.pool[*id] else { unreachable!() };
                let mut set: HashSet<FiltersetId> = HashSet::from_iter(items.iter().copied());
                for ptr in inner_ors.iter() {
                    set.remove(ptr);
                    let Filterset::Or(ref others) = self.pool[items[*ptr]] else { unreachable!() };
                    set.extend(others);
                }
                let Filterset::Or(ref mut items) = self.pool[*id] else { unreachable!() };
                items.clear();
                items.extend(set);
                for ptr in inner_ors {
                    self.pool[*ptr] = Filterset::Dead;
                }
            }
            RewriteAction::EliminateSingleOr(id) => {
                let Filterset::Or(srcs) = std::mem::replace(&mut self.pool[*id], Filterset::Dead)
                else {
                    unreachable!()
                };
                self.pool.swap(*id, srcs[0]);
            }
            RewriteAction::EliminateSingleAnd(id) => {
                let Filterset::And(srcs) = std::mem::replace(&mut self.pool[*id], Filterset::Dead)
                else {
                    unreachable!()
                };
                self.pool.swap(*id, srcs[0]);
            }
            RewriteAction::EliminateNotNot(not1p, not2p, innerp) => {
                self.pool[*not1p] = std::mem::replace(&mut self.pool[*innerp], Filterset::Dead);
                self.pool[*not2p] = Filterset::Dead;
            }
            RewriteAction::DnfDnf(dnf1, dnf2, src2) => {
                let (c2, _) = self.dead_and_take_dnf(*dnf2);
                let Filterset::RelDnf(ref mut c1, ref mut src1) = self.pool[*dnf1] else {
                    unreachable!()
                };
                // TODO: we could reuse an allocation here, for example by copying c2 to c1 first,
                // doing the cartesian product on subranges of c1 and collecting to c2, then
                // replacing the vector of dnf1. meh.
                let new_clauses: Vec<Vec<PredicateId>> = c1
                    .iter()
                    .cartesian_product(c2)
                    .map(|(cl1, cl2)| {
                        cl1.iter().chain(cl2.iter()).cloned().collect::<Vec<PredicateId>>()
                    })
                    .collect();
                *c1 = new_clauses;
                *src1 = *src2;
            }
            RewriteAction::MergeDnfsInOr(or, dnfs_by_source) => {
                use Filterset::Dead;
                let Filterset::Or(cs) = std::mem::replace(&mut self.pool[*or], Dead) else {
                    unreachable!()
                };
                let mut or_clauses: HashSet<FiltersetId> = HashSet::from_iter(cs.iter().copied());
                for (source, dnfs) in dnfs_by_source.iter() {
                    if dnfs.len() < 2 {
                        continue;
                    }
                    let (mut firstc, _) = self.dead_and_take_dnf(dnfs[0]);
                    for dnf in dnfs.iter().skip(1) {
                        let (c, _) = self.dead_and_take_dnf(*dnf);
                        or_clauses.remove(dnf);
                        firstc.extend(c.into_iter());
                    }
                    self.pool[dnfs[0]] = Filterset::RelDnf(firstc, *source);
                }
                self.pool[*or] = Filterset::Or(or_clauses.into_iter().collect());
            }
            RewriteAction::MergeDnfsInAnd(and, dnfs_by_source) => {
                use Filterset::Dead;
                let Filterset::And(cs) = std::mem::replace(&mut self.pool[*and], Dead) else {
                    unreachable!()
                };
                let mut and_clauses: HashSet<FiltersetId> = HashSet::from_iter(cs);
                for dnfs in dnfs_by_source.values() {
                    if dnfs.len() < 2
                        || self.len_of_merged_dnf(dnfs.iter().copied())
                            > DNFS_IN_AND_MERGE_MAX_CLAUSES
                    {
                        continue;
                    }
                    let new_clause_list: Vec<Vec<PredicateId>> = dnfs
                        .iter()
                        .filter_map(|x| match &self.pool[*x] {
                            Filterset::RelDnf(items, _) => Some(items.iter()),
                            _ => None,
                        })
                        .multi_cartesian_product()
                        .map(|combo| combo.into_iter().flatten().copied().collect())
                        .collect();
                    let Filterset::RelDnf(firstc, _) = &mut self.pool[dnfs[0]] else {
                        unreachable!()
                    };
                    *firstc = new_clause_list;

                    for dnf in dnfs.iter().skip(1) {
                        let _ = self.dead_and_take_dnf(*dnf);
                        and_clauses.remove(dnf);
                    }
                }
                self.pool[*and] = Filterset::And(and_clauses.into_iter().collect());
            }
        }
    }

    /// Get a post-order (inverse topo-order) via DFS.
    /// The second return value is a lookup table that yields parent_of[x]
    /// (which we'll use later)
    ///
    /// TODO: we could also track this when puhsing stuff into the evaluator (since you need
    /// referenes to inner objects, its effectively already a postorder), but that's too much
    /// work for now
    pub fn post_order(&mut self, root: FiltersetId) -> (Vec<FiltersetId>, Vec<FiltersetId>) {
        let mut stack1 = vec![root];
        let mut stack2 = Vec::with_capacity(self.pool.len());
        let mut parent_of = vec![usize::MAX; self.pool.len()]; // infinity = unknown
        // I don't think we need to track visited for a forest?
        // if something is on the stack, it is popped before its children are inserted,
        // and the children won't put it on the stack again.
        //let mut visited = HashSet::new();
        while let Some(v) = stack1.pop() {
            //   if visited.insert(v) {
            //       continue;
            //   }
            stack2.push(v);
            match self.pool[v].children() {
                ChildrenRef::None => continue,
                ChildrenRef::One(x) => {
                    stack1.push(x);
                    parent_of[x] = v;
                }
                ChildrenRef::Many(items) => {
                    stack1.extend(items);
                    for item in items {
                        parent_of[*item] = v;
                    }
                }
            }
        }
        stack2.reverse();
        (stack2, parent_of)
    }

    pub fn normalize(&mut self, root: FiltersetId) {
        if !self.results.is_empty() {
            panic!("Normalizing after there are results is unsafe");
        }
        let mut worklist = VecDeque::with_capacity(self.pool.len());
        let (post_order, parent_of) = self.post_order(root);
        worklist.extend(post_order.iter().copied());

        pub fn inner<T>(
            this: &mut Evaluator<T>, x: FiltersetId, worklist: &mut VecDeque<FiltersetId>,
            parent_of: &[usize], root: FiltersetId,
        ) {
            // reach a local fixpoint before queuing parent
            let mut any_action = false;
            while !matches!(this.rewrite_one(x), RewriteAction::None) {
                any_action = true;
            }
            if any_action && x != root {
                let parent = parent_of[x];
                if parent == usize::MAX {
                    panic!("Don't know parent of {x} even though it was rewritten. This is a bug.");
                }
                worklist.push_back(parent);
            }
        }

        // While there were children rewritten, rewrite the parents (so rewrite until there are no
        // changes left)
        while let Some(x) = worklist.pop_front() {
            inner(self, x, &mut worklist, &parent_of, root);
        }
    }

    /// For good performance, you must normalize() first.
    /// Guarantees that `results[id]` will exist.
    /// WARNING: because of how Not() is implemented, the Roaring in results[id] might contain ids
    /// beyond the end of the actual data. Please clamp it to your actual data ID range.
    pub fn materialize(&mut self, matcher: &impl Matcher<T>, id: FiltersetId) {
        let mut stack = vec![(id, false)];
        // "two-phase scheduling" algorithm. a node can either be "ready", meaning we can materialize it right
        // away, or "unready" which means we need to materialize its children first.
        // at first, we put (root, unready) on the stack.
        // when popping a node (v, unready):
        //   push (v,ready) on the stack to visit it eventually
        //   if it has children: push all children with (u, unready)
        // (so if there are no children, it will be materialized in the next round)
        // when popping a node (v,ready):
        //   we can assume all the children of v are already materialized.
        //   materialize v based on these.
        while let Some((node, ready)) = stack.pop() {
            if !ready {
                stack.push((node, true));
                match self.pool[node].children() {
                    ChildrenRef::None => (),
                    ChildrenRef::One(x) => {
                        stack.push((x, false));
                    }
                    ChildrenRef::Many(items) => {
                        for item in items {
                            stack.push((*item, false));
                        }
                    }
                }
                continue;
            }
            // ready to materialize.
            match &self.pool[node] {
                Filterset::Dead => {
                    eprintln!("Tried to materialize Dead. In the future, this may panic.");
                    self.results.insert(node, Roaring::new());
                }
                Filterset::Primitive(bm) => {
                    self.results.insert(node, bm.clone());
                }
                Filterset::BlackBox(src) => {
                    let source_result = &self.results[src];
                    self.results.insert(node, source_result.clone());
                }
                Filterset::And(items) => {
                    self.results.insert(node, items.iter().map(|x| &self.results[x]).union());
                }
                Filterset::Or(items) => {
                    self.results.insert(node, items.iter().map(|x| &self.results[x]).union());
                }
                Filterset::Not(src) => {
                    let source_result = &self.results[src];
                    // TODO: I didn't find a flip operation on RoaringBitmap, there isn't one in
                    // roaring-rs, but there is one in croaring. Investigate the performance of
                    // switching to croaring.
                    // WARN: this is a bug: since we don't know the data len, this *will* include
                    // records beyond the actual record count.
                    self.results.insert(node, Roaring::full() - source_result);
                }
                Filterset::RelDnf(items, src) => {
                    let source_result = &self.results[src];
                    let this_result = matcher.subset_matching_dnf(
                        items.iter().map(|x| x.iter().map(|y| &self.predicates[*y])),
                        source_result,
                    );

                    self.results.insert(node, this_result);
                }
            }
        }
    }
}

impl<T: Debug> Evaluator<T> {
    /// Pretty-print the graph in GraphViz .dot
    pub fn dot(&mut self, root: FiltersetId) -> String {
        let mut out = String::from("digraph D {\n");
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            let node = format!("{:?}", &self.pool[v]).replace('"', "'");
            writeln!(out, "  n{v} [label=\"{node}\"];").ok();
            match self.pool[v].children() {
                ChildrenRef::None => (),
                ChildrenRef::One(a) => {
                    stack.push(a);
                    writeln!(out, "  n{v} -> n{a};").ok();
                }
                ChildrenRef::Many(items) => {
                    for item in items {
                        writeln!(out, "  n{v} -> n{item};").ok();
                    }
                    stack.extend(items);
                }
            }
        }
        out.push('}');
        out
    }
}

impl<T> Default for Evaluator<T> {
    fn default() -> Self {
        Self::new()
    }
}
pub trait Matcher<T> {
    /// Note: for good performance, you SHOULD implement [Matcher::subset_matching_dnf], as the default
    /// implementation calls this a lot, generating lots of slow scans.
    fn subset_matching(&self, predicate: &Predicate<T>, input: &Roaring) -> Roaring;
    fn subset_matching_dnf<'a, O, I>(&self, predicates: O, input: &Roaring) -> Roaring
    where
        O: Iterator<Item = I>,
        I: Iterator<Item = &'a Predicate<T>>,
        T: 'a,
    {
        predicates.map(|x| x.map(|y| self.subset_matching(y, input)).intersection()).union()
    }
}
pub struct YesManMatcher();
impl<T> Matcher<T> for YesManMatcher {
    fn subset_matching(&self, _: &Predicate<T>, input: &Roaring) -> Roaring {
        input.clone()
    }
}
