use roaring::{MultiOps, RoaringBitmap as Roaring};
use std::collections::HashMap;
use std::fmt::{Debug, Write};
use std::{
    cmp::Ordering,
    collections::{HashSet, VecDeque},
    mem,
};

pub type FiltersetId = usize;
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
pub enum Filterset<T> {
    Dead,
    Primitive(Roaring),
    Rel(Predicate<T>, FiltersetId),
    RelIntersect(Vec<Predicate<T>>, FiltersetId),
    RelUnion(Vec<Predicate<T>>, FiltersetId),
    And(Vec<FiltersetId>),
    Or(Vec<FiltersetId>),
    Not(FiltersetId),
}
pub enum RewriteAction {
    None,
    // Pointer of outer and, and indices to inner ands in its item list
    CompressAnd(FiltersetId, Vec<FiltersetId>),
    CompressOr(FiltersetId, Vec<FiltersetId>),
    EliminateNotNot(FiltersetId, FiltersetId, FiltersetId),
    /// First Rel, Second Rel, NewArg
    NestedRelToIntersect(FiltersetId, FiltersetId, FiltersetId),
    /// First Rel, nested RelIntersect, NewArg
    ParentRelToIntersect(FiltersetId, FiltersetId, FiltersetId),
    /// Parent RelIntersect, nested Rel, nested Rel arg = new arg
    CompressRelInRelIntersect(FiltersetId, FiltersetId, FiltersetId),
    /// Parent RelIntersect, nested RelIntersect, NewArg
    CompressRelIntersect(FiltersetId, FiltersetId, FiltersetId),
    /// Parent RelUnion, nested RelUnion, NewArg
    CompressRelUnion(FiltersetId, FiltersetId, FiltersetId),
}
pub enum ChildrenRef<'a> {
    None,
    One(FiltersetId),
    Many(&'a [FiltersetId]),
}

pub struct Evaluator<M: Matcher<T>, T> {
    pub pool: Vec<Filterset<T>>,
    pub results: HashMap<FiltersetId, Roaring>,
    pub matcher: M,
}
impl<M: Matcher<T>, T> Evaluator<M, T> {
    pub fn from_matcher(matcher: M) -> Self {
        Self { pool: vec![], results: HashMap::new(), matcher }
    }
    pub fn is_and(&self, id: FiltersetId) -> bool {
        matches!(self.pool[id], Filterset::And(_))
    }
    pub fn is_or(&self, id: FiltersetId) -> bool {
        matches!(self.pool[id], Filterset::Or(_))
    }
    /// Returns the action which ended up being executed
    pub fn rewrite_one(&mut self, id: FiltersetId) -> RewriteAction {
        let mut action = RewriteAction::None;
        match &self.pool[id] {
            Filterset::And(items) => {
                let ands: Vec<FiltersetId> =
                    items.iter().copied().filter(|p| self.is_and(*p)).collect();
                if !ands.is_empty() {
                    action = RewriteAction::CompressAnd(id, ands);
                }
            }
            Filterset::Or(items) => {
                let ors: Vec<usize> = items.iter().copied().filter(|x| self.is_or(*x)).collect();
                if !ors.is_empty() {
                    action = RewriteAction::CompressOr(id, ors);
                }
            }
            Filterset::Not(y) => {
                if let Filterset::Not(q) = &self.pool[*y] {
                    action = RewriteAction::EliminateNotNot(id, *y, *q)
                }
            }
            Filterset::Rel(_pred, arg) => match &self.pool[*arg] {
                Filterset::Rel(_pred2, arg2) => {
                    action = RewriteAction::NestedRelToIntersect(id, *arg, *arg2);
                }
                Filterset::RelIntersect(_preds, arg2) => {
                    action = RewriteAction::ParentRelToIntersect(id, *arg, *arg2);
                }
                _ => (),
            },
            Filterset::RelIntersect(_preds, arg) => match &self.pool[*arg] {
                Filterset::Rel(_pred2, arg2) => {
                    action = RewriteAction::CompressRelInRelIntersect(id, *arg, *arg2);
                }
                Filterset::RelIntersect(_pred2, arg2) => {
                    action = RewriteAction::CompressRelIntersect(id, *arg, *arg2);
                }
                _ => (),
            },
            Filterset::RelUnion(_preds, arg) => {
                if let Filterset::RelUnion(_preds2, arg2) = &self.pool[*arg] {
                    action = RewriteAction::CompressRelUnion(id, *arg, *arg2);
                }
            }
            _ => (),
        }
        self.do_rewrite_action(&action);
        action
    }
    /// Very important invariant: we assume anyone who has the index of a Filterset "owns" it,
    /// so we cannot create dangling references (bad references to Dead values) by rewriting.
    pub fn do_rewrite_action(&mut self, action: &RewriteAction) {
        use Filterset::Dead;
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
                let Filterset::And(ref mut items) = self.pool[*id] else { unreachable!() };
                items.clear();
                items.extend(set);
                for ptr in inner_ors {
                    self.pool[*ptr] = Filterset::Dead;
                }
            }
            RewriteAction::EliminateNotNot(not1p, not2p, innerp) => {
                self.pool[*not1p] = std::mem::replace(&mut self.pool[*innerp], Filterset::Dead);
                self.pool[*not2p] = Filterset::Dead;
            }
            RewriteAction::NestedRelToIntersect(r1, r2, rel2src) => {
                let Filterset::Rel(pred1, _) = std::mem::replace(&mut self.pool[*r1], Dead) else {
                    unreachable!()
                };
                let Filterset::Rel(pred2, _) = std::mem::replace(&mut self.pool[*r2], Dead) else {
                    unreachable!()
                };
                self.pool[*r1] = Filterset::RelIntersect(vec![pred1, pred2], *rel2src);
            }
            RewriteAction::ParentRelToIntersect(rel, ist, intersrc) => {
                let Filterset::Rel(pred0, _) = std::mem::replace(&mut self.pool[*rel], Dead) else {
                    unreachable!()
                };
                let Filterset::RelIntersect(mut ps, _) = mem::replace(&mut self.pool[*ist], Dead)
                else {
                    unreachable!()
                };
                ps.push(pred0); // TODO: maybe push_first for better selectivity?
                self.pool[*rel] = Filterset::RelIntersect(ps, *intersrc);
            }
            RewriteAction::CompressRelInRelIntersect(ist, rel, relsrc) => {
                let Filterset::Rel(pred, _) = std::mem::replace(&mut self.pool[*rel], Dead) else {
                    unreachable!()
                };
                let Filterset::RelIntersect(ps, arg) = &mut self.pool[*ist] else { unreachable!() };
                ps.push(pred);
                *arg = *relsrc;
            }
            RewriteAction::CompressRelIntersect(ist1, ist2, arg2) => {
                let Filterset::RelIntersect(ps2, _) =
                    std::mem::replace(&mut self.pool[*ist2], Dead)
                else {
                    unreachable!()
                };
                let Filterset::RelIntersect(ps1, a1) = &mut self.pool[*ist1] else {
                    unreachable!()
                };
                ps1.extend(ps2);
                *a1 = *arg2;
            }
            RewriteAction::CompressRelUnion(u1, u2, a2) => {
                let Filterset::RelUnion(ps2, _) = std::mem::replace(&mut self.pool[*u2], Dead)
                else {
                    unreachable!()
                };
                let Filterset::RelIntersect(ps1, a1) = &mut self.pool[*u1] else { unreachable!() };
                ps1.extend(ps2);
                *a1 = *a2;
            }
        }
    }

    pub fn children(&'_ self, id: FiltersetId) -> ChildrenRef<'_> {
        match &self.pool[id] {
            Filterset::Dead | Filterset::Primitive(_) => ChildrenRef::None,
            Filterset::Rel(_, a)
            | Filterset::RelIntersect(_, a)
            | Filterset::RelUnion(_, a)
            | Filterset::Not(a) => ChildrenRef::One(*a),
            Filterset::And(i) | Filterset::Or(i) => ChildrenRef::Many(i),
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
            match self.children(v) {
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
        let mut worklist = VecDeque::new();
        let (post_order, parent_of) = self.post_order(root);

        // First, scan the entire tree from the leaves up (by a postorder), and try to simplify.
        // If we rewrote something, mark the parent for rewriting too.
        pub fn inner<M: Matcher<T>, T>(
            this: &mut Evaluator<M, T>, x: FiltersetId, worklist: &mut VecDeque<FiltersetId>,
            parent_of: &[usize], root: FiltersetId,
        ) {
            let action_taken = this.rewrite_one(x);
            if !matches!(action_taken, RewriteAction::None) && x != root {
                let parent = parent_of[x];
                if parent == usize::MAX {
                    panic!("Don't know parent of {x} even though it was rewritten. This is a bug.");
                }
                worklist.push_back(parent);
            }
        }
        for x in post_order {
            inner(self, x, &mut worklist, &parent_of, root);
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
    pub fn materialize(&mut self, id: FiltersetId) {
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
                match self.children(node) {
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
                Filterset::Rel(predicate, src) => {
                    let source_result = &self.results[src];
                    let matches = self.matcher.subset_matching(predicate, source_result);
                    self.results.insert(node, matches);
                }
                Filterset::RelIntersect(predicates, src) => {
                    let source_result = &self.results[src];
                    let matches = self.matcher.subset_matching_all(predicates, source_result);
                    self.results.insert(node, matches);
                }
                Filterset::RelUnion(predicates, src) => {
                    let source_result = &self.results[src];
                    let matches = self.matcher.subset_matching_either(predicates, source_result);
                    self.results.insert(node, matches);
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
            }
        }
    }
}

impl<M: Matcher<T>, T: Debug> Evaluator<M, T> {
    /// Pretty-print the graph in GraphViz .dot
    pub fn dot(&mut self, root: FiltersetId) -> String {
        let mut out = String::from("digraph D {\n");
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            let node = format!("{:?}", &self.pool[v]).replace('"', "'");
            writeln!(out, "  n{v} [label=\"{node}\"];").ok();
            let children = self.children(v);
            match children {
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
pub trait Matcher<T> {
    fn subset_matching(&self, predicate: &Predicate<T>, input: &Roaring) -> Roaring;
    fn subset_matching_all(&self, predicates: &[Predicate<T>], input: &Roaring) -> Roaring {
        predicates.iter().map(|x| self.subset_matching(x, input)).intersection()
    }
    fn subset_matching_either(&self, predicates: &[Predicate<T>], input: &Roaring) -> Roaring {
        predicates.iter().map(|x| self.subset_matching(x, input)).union()
    }
}
pub struct YesManMatcher();
impl<T> Matcher<T> for YesManMatcher {
    fn subset_matching(&self, _: &Predicate<T>, input: &Roaring) -> Roaring {
        input.clone()
    }
}
